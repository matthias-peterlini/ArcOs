# arcOS

arcOS è un sistema operativo per PC x86-64 scritto da zero. Non deriva da
Linux, da Unix, da DOS o da Windows, e non ne imita le interfacce: boot
loader, kernel, file system, formato dei driver, chiamate di sistema e shell
sono progettati apposta. Il codice è Rust `no_std` (più due piccoli stadi di
avvio in assembly) e non usa librerie esterne: niente crate di terze parti,
solo il compilatore e la libreria `core`/`alloc` di Rust. Il linguaggio
ArcScript viene dal suo repository, portato senza `std` anche lui.

```
  arcOS 0.1.0
  QEMU Virtual CPU version 2.5+ · 1024x768 framebuffer · loader: UEFI

[    0.047] info  fs: mounted sys: (System, disk0p1)
[    0.050] info  drivers: nvme 1.0.0 installed for Red Hat (QEMU) NVMe controller (pci 00:03.0)
[    0.065] info  drivers: ps2kbd 1.0.0 installed for PS/2 keyboard (platform ps2-keyboard)
[    0.113] info  drivers: e1000 1.0.0 installed for Intel Ethernet controller (pci 00:02.0)
Type help for the list of commands.

sys:/ › hello world
Hello from user mode!
```

## Cosa c'è

- **Avvio BIOS e UEFI dalla stessa immagine.** ArcBoot per BIOS (MBR +
  secondo stadio, in assembly) e ArcBoot per UEFI (un'applicazione EFI in
  Rust, con l'interfaccia UEFI dichiarata a mano). Entrambi passano al kernel
  la stessa struttura `BootInfo`.
- **Kernel** a 64 bit: memoria virtuale a 4 livelli, allocatore di pagine e
  heap, scheduler preemptivo a 1 kHz, thread del kernel, code di attesa,
  console su framebuffer (con un font proprio) o testo VGA, più la porta
  seriale.
- **Driver installabili, che si installano da soli.** Ogni driver è un
  pacchetto `.adp` (modulo + manifesto). Il kernel confronta i manifesti con
  l'hardware trovato e carica il driver più adatto, senza comandi:
  all'avvio, dopo ogni scansione dei dispositivi e quando un pacchetto viene
  lasciato in `sys:/inbox`. I driver si possono anche aggiornare e rimuovere
  a sistema avviato. Dettagli in [docs/driver.md](docs/driver.md).
- **Driver inclusi** (elenco completo in [docs/driver.md](docs/driver.md#hardware-supportato)):
  - dischi IDE/ATA, AHCI (SATA), NVMe, virtio-blk;
  - USB con tutti e quattro i tipi di controller (xHCI, EHCI, UHCI, OHCI),
    hub, tastiere e mouse (HID), chiavette e dischi esterni; i dispositivi
    si possono collegare e scollegare a sistema avviato;
  - rete Intel PRO/1000 (e1000, 8254x/8257x), Realtek RTL8139 e
    RTL8169/8111, AMD PCnet, virtio-net;
  - tastiera e mouse PS/2.
  Interruzioni MSI/MSI-X dove il dispositivo le offre, e più schede dello
  stesso tipo insieme.
- **ArcFS**, il file system nativo: blocchi da 4 KiB, extent, nomi
  indipendenti da maiuscole/minuscole, controllo di coerenza (`check`).
  Volumi con nome: `sys:` (il disco di sistema), `boot:` (il pacchetto di
  avvio), `ram:` (in memoria).
- **Programmi in modalità utente** (ring 3), ognuno nel proprio spazio di
  indirizzi, con chiamate di sistema basate su handle. Un programma che
  sbaglia viene fermato senza toccare il resto; Ctrl+C interrompe quello in
  primo piano. Programmi inclusi: `hello`, `info`, `calc`, `edit`, `snake`.
  Vedi [docs/programmi.md](docs/programmi.md).
- **ArcScript**, il linguaggio applicativo di arcOS, con il suo compilatore
  e la sua macchina virtuale: `arcc` compila ed esegue i sorgenti `.as` e i
  pacchetti `.arc`, che la shell avvia come qualsiasi programma (`hello.as`,
  `notes add …`). Le interfacce (`view`) per ora sono mostrate come testo.
  Vedi [docs/arcscript.md](docs/arcscript.md).
- **Programmi Linux e Windows** per x86-64, eseguiti senza modificarli: un
  eseguibile ELF statico (musl) è un programma Linux, servito da un livello di
  compatibilità nel kernel; un `.exe` è un programma Windows, caricato da
  `winrun` con un `kernel32`/`msvcrt` minimi. `install` copia programmi liberi
  già pronti dal negozio `sys:/store` (BusyBox, Lua). Vedi
  [docs/compat.md](docs/compat.md).
- **Rete**: ARP, IPv4, ICMP (ping), UDP, client DHCP e DNS.
- **Shell** con cronologia, completamento con Tab e una quarantina di comandi
  (`help` li elenca).

L'architettura è descritta in [docs/architettura.md](docs/architettura.md).

## Scaricarlo

Nelle [release](https://github.com/matthias-peterlini/ArcOs/releases) ci
sono il **CD live** `arcos.iso`, da avviare in una macchina virtuale
(VirtualBox, VMware, QEMU: `qemu-system-x86_64 -m 512M -cdrom arcos.iso`)
o da un CD, e l'**immagine di disco** `arcos-img.zip`, da scrivere su una
chiavetta USB per un PC vero (vedi sotto). Le costruisce il workflow
`.github/workflows/release.yml` a ogni tag `v*`, o a mano dalla scheda
Actions.

## Compilare

Servono:

- Rust stable (la versione e i target giusti li installa `rustup` da solo,
  grazie a `rust-toolchain.toml`);
- `nasm` per i due stadi di avvio BIOS;
- `qemu-system-x86_64` per provarlo;
- OVMF (pacchetto `ovmf`) per provarlo con firmware UEFI.

```sh
make            # costruisce build/arcos.img (128 MiB, avviabile BIOS e UEFI)
make iso        # costruisce build/arcos.iso, un CD live (BIOS e UEFI)
make run-iso    # avvia il CD live in QEMU, senza disco rigido
make run        # lo avvia in QEMU (PC classico, disco IDE)
make run-uefi   # lo avvia in QEMU con firmware UEFI
make run-q35    # PC moderno, disco SATA (AHCI)
make run-nvme   # disco NVMe
make test       # avvio senza schermo con una sessione di comandi scritta
make test-uefi  # la stessa, avviata da UEFI
make test-iso   # la stessa sul CD live, con BIOS e con UEFI
make test-drivers  # un avvio per ogni disco, scheda di rete e controller
                # USB emulato da QEMU, controllando che il driver funzioni
make test-arcscript  # programmi ArcScript eseguiti su arcOS, con l'uscita
                # controllata
make test-compat  # un programma Linux e uno Windows eseguiti su arcOS,
                # con l'uscita controllata
scripts/build-qemu.sh  # un QEMU con i modelli di tools/qemu (RTL8169 e
                # RTL8168), usato da make test-drivers per provarne i driver
make check      # test delle librerie condivise e dello strumento host
```

`make SIZE=1G` cambia la dimensione dell'immagine, `make LIVE_SIZE=64M`
quella del volume di sistema del CD live, `make MEMORY=2G` la memoria della
macchina virtuale.

Il CD live non ha bisogno di un driver per i lettori CD: i loader (BIOS e
UEFI) leggono dal CD, oltre al kernel e al pacchetto di avvio, anche il
volume di sistema, che il kernel monta dalla memoria come `sys:`. Si può
scrivere e installare driver come sempre, ma tutto si perde allo
spegnimento.

## Su un PC vero

`build/arcos.img` è un'immagine di disco completa: scritta su una chiavetta
USB (per esempio con `dd`, facendo molta attenzione a scegliere il
dispositivo giusto) si avvia sia in modalità BIOS/legacy sia in UEFI. Il
loader non è firmato, quindi Secure Boot va disattivato.

Il disco di sistema può essere la chiavetta stessa: i driver USB sono nel
pacchetto di avvio e il kernel aspetta i dischi USB prima di cercare
`sys:`. Tastiere e mouse USB funzionano senza l'emulazione PS/2 del
firmware.

Da sapere prima di provarlo:

- il kernel usa un solo processore;
- non ci sono ancora driver per Wi-Fi, schede video (si usa il framebuffer
  del firmware), audio, né per le schede di rete Intel I225/I226 e
  Realtek RTL8125 (2,5 Gbit);
- alcuni driver sono scritti dalla documentazione dei chip e non sono
  ancora stati provati su hardware vero perché QEMU non li emula; per le
  Realtek RTL8169/8111 c'è un modello scritto apposta. Lo stato di ognuno
  è in [docs/driver.md](docs/driver.md#hardware-supportato).

Per un disco dati in più (una chiavetta da usare con arcOS):

```sh
tools/target/release/arctool mkfs chiavetta.img --size 1G --label Dati
```

crea un'immagine con una partizione ArcFS, che arcOS monta come `dati:`.

## Struttura del repository

| Percorso | Contenuto |
|---|---|
| `boot/bios/` | ArcBoot per BIOS: `stage1.asm` (MBR) e `stage2.asm` |
| `boot/uefi/` | ArcBoot per UEFI (`BOOTX64.EFI`) |
| `system/kernel/` | il kernel |
| `system/libs/arcdk/` | Arc Development Kit: ABI dei driver, manifesti, pacchetti, chiamate di sistema |
| `system/libs/arcdrv/` | libreria di supporto per scrivere driver |
| `system/libs/arcrt/` | libreria di supporto per scrivere programmi |
| `system/libs/arcfs/` | ArcFS, condiviso tra kernel e strumenti host |
| `system/libs/arcusb/` | il nucleo USB comune ai driver dei controller (enumerazione, hub, collegamento a caldo) |
| `system/libs/arcvirtio/` | interfaccia virtio (legacy e moderna) comune ai driver virtio |
| `system/libs/arcc-core/`, `system/libs/arc-vm/` | compilatore e macchina virtuale di ArcScript |
| `system/drivers/` | i driver, uno per cartella, ognuno con il suo `driver.manifest` |
| `system/apps/` | i programmi |
| `tools/arctool/` | strumento host: crea pacchetti e immagini, legge e scrive volumi ArcFS |
| `tools/qemu/` | modelli di dispositivi per QEMU che QEMU non ha (Realtek RTL8169/8168) |
| `rootfs/` | file copiati nel volume di sistema |
| `scripts/` | test automatici in QEMU (`test.sh`, `test-drivers.sh`, `test-arcscript.sh`) e `build-qemu.sh` |

## Primi passi nella shell

```
help                   tutti i comandi
dev                    dispositivi trovati e il loro driver
drv                    driver disponibili, installati, in esecuzione
drv info e1000         manifesto e stato di un driver
vol / disks            volumi montati, dischi e partizioni
ls sys:/system/drivers il magazzino dei driver
cat sys:/home/welcome.txt
edit sys:/home/nota.txt
net / ping 10.0.2.2    rete
info / snake / calc    programmi
cd sys:/home/arcscript
hello.as               un programma ArcScript
arcc help              compilare ed eseguire ArcScript
kbd it                 tastiera italiana
shutdown / restart
```

## Licenza

Proprietaria: vedi [LICENSE](LICENSE).
