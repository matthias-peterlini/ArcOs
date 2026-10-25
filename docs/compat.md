# Programmi Linux e Windows su arcOS

arcOS esegue, oltre ai suoi programmi nativi e a quelli ArcScript, anche
programmi **Linux** e **Windows** per x86-64, senza modificarli. Non c'è una
macchina virtuale: il programma gira nello stesso modo degli altri, in ring 3
e nel suo spazio di indirizzi; quando chiede qualcosa al sistema, arcOS traduce
la richiesta nelle proprie chiamate.

L'idea è la stessa di [Wine](https://www.winehq.org/) e di
[WSL](https://learn.microsoft.com/windows/wsl/), in piccolo: nessun codice di
Microsoft o di Linux è copiato in arcOS. Il supporto è scritto da zero a
partire dalle interfacce pubbliche (la ABI di System V per Linux, la
convenzione x64 e i pochi `.dll` di sistema per Windows).

## Programmi Linux

arcOS carica un eseguibile ELF x86-64 **collegato staticamente** (senza
librerie condivise: niente `ld.so`) e serve le chiamate di sistema Linux che
fa. Funziona con i programmi costruiti con [musl](https://musl.libc.org/), per
esempio `musl-gcc -static`.

Un programma Linux si avvia come gli altri, per percorso o per nome:

```
sys:/home/hello            un eseguibile Linux qualunque
busybox ls -la             un programma installato (vedi "Installare programmi")
```

### Cosa vede il programma

| Area | Su arcOS |
|---|---|
| `stdin`, `stdout`, `stderr` | la console; `stdin` legge una riga per volta, con l'eco dei tasti |
| file e cartelle | i volumi montati: il "/" di Linux è la radice del volume corrente, così `/home/x` è `<volume>:/home/x` |
| memoria | `brk` e `mmap` anonimo (il programma alloca e libera normalmente) |
| orologio | `clock_gettime`, `gettimeofday`, `nanosleep` |
| TLS | `arch_prctl(ARCH_SET_FS)` imposta la base del segmento FS del thread |
| casualità | `getrandom` |
| rete, utenti, segnali, processi | non ancora: le chiamate rispondono con un valore innocuo o `ENOSYS` |

Le chiamate implementate bastano ai programmi da riga di comando: lettura e
scrittura di file, elenco di cartelle (`getdents64`), `stat`, `ioctl` per sapere
se l'uscita è un terminale, e così via. Una chiamata non ancora prevista compare
nel giornale di sistema come `unimplemented syscall N` (con `verbose on`), così
è facile vedere cosa manca.

Com'è fatto: `system/kernel/src/linux.rs` (le chiamate) e
`system/kernel/src/arch/syscall.rs` (l'istruzione `syscall` e le basi FS/GS di
ogni thread).

## Programmi Windows

I programmi Windows li esegue `winrun`, un programma di arcOS
(`system/apps/winrun`). `winrun` carica l'eseguibile `.exe` (PE32+ per x64),
mette i propri indirizzi nella tabella delle importazioni al posto delle
funzioni di `kernel32.dll` e `msvcrt.dll`, prepara un blocco di thread (TEB) e
salta al punto d'ingresso. Il programma gira nello stesso processo di `winrun`:
quando chiama, per esempio, `printf`, chiama il codice di `winrun`, che a sua
volta usa le chiamate di arcOS.

Un `.exe` si avvia per nome (arcOS riconosce l'intestazione `MZ` e usa `winrun`
da solo) o esplicitamente:

```
hello.exe                  arcOS vede che è un programma Windows e lo avvia
winrun hello.exe alpha     lo stesso, scritto per esteso
```

Sono gestiti i **programmi da console per x64** costruiti con
[MinGW-w64](https://www.mingw-w64.org/) (`x86_64-w64-mingw32-gcc`). Di
`kernel32`/`msvcrt` è implementato ciò che serve a questi programmi: i tre
flussi standard, la famiglia `printf` (con un motore di formattazione scritto
in `winrun`), la memoria (`malloc`/`HeapAlloc`/`VirtualAlloc`), gli argomenti
(`__getmainargs`), l'avvio del runtime C (`_initterm`) e stub innocui per il
resto. Manca tutto il resto di Windows: niente interfaccia grafica, registro,
processi o thread veri.

I programmi a 32 bit e quelli che usano la grafica non sono ancora gestiti:
l'obiettivo è partire dalla console a 64 bit e crescere da lì.

Com'è fatto: `system/apps/winrun/src/pe.rs` (il caricatore) e
`system/apps/winrun/src/win.rs` (le funzioni di Windows e il `printf`).

## Installare programmi

Il volume di sistema porta un piccolo "negozio" in `sys:/store`: programmi
liberi già compilati (vedi `sys:/store/SOURCES.md` per sorgenti e licenze). Il
comando `install` li copia in `sys:/apps`, da dove si avviano per nome:

```
install                    elenca i programmi disponibili
install busybox            installa BusyBox (decine di comandi Unix)
install lua                installa l'interprete Lua
```

[BusyBox](https://busybox.net/) è un programma solo che, a seconda del nome con
cui lo si chiama, diventa uno di decine di comandi Unix. Dopo `install busybox`,
arcOS mette a disposizione quei comandi (`cat`, `grep`, `wc`, `seq`, `sort`,
`ash`…): digitandone uno, la shell avvia `busybox` passandogli quel nome. I
comandi propri di arcOS (per esempio `ls`) hanno la precedenza.

## Provarlo

`make test-compat` compila al volo un piccolo programma Linux (con `musl-gcc`) e
uno Windows (con `x86_64-w64-mingw32-gcc`), li mette su una copia dell'immagine
insieme ai programmi del negozio, avvia arcOS e controlla che funzionino. Se uno
dei due compilatori non c'è, quella metà viene saltata (e segnalata).

Per compilare a mano un programma da provare:

```
musl-gcc -static -Os -o hello hello.c            # Linux
x86_64-w64-mingw32-gcc -O2 -o hello.exe hello.c  # Windows
```

poi copiarlo nell'immagine con `arctool fs` (vedi `docs/programmi.md`).
