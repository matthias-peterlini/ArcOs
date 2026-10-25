# L'interfaccia grafica

Quando il loader trova uno schermo grafico di almeno 800x600, arcOS si avvia
in modalità grafica. Sullo schermo non compare mai testo tecnico: il kernel
scrive i suoi messaggi solo sulla porta seriale e nel giornale di sistema, e
le righe di comando si vedono soltanto aprendo il Terminale. Anche un errore
fatale mostra una schermata semplice («arcOS si è dovuto fermare»), con i
dettagli solo sulla porta seriale.

## Avvio

1. **Splash**: logo, nome e una barra che avanza mentre il kernel avvia le sue
   parti (memoria, dischi, driver, rete) e carica icone e sfondi.
2. **Primo avvio**: una configurazione guidata in cinque passi: *Benvenuto*
   (pulsante **INIZIA**), *Come ti chiami?*, *Scegli la tastiera*, *Scegli lo
   sfondo*, *Tutto pronto*. Si usa con il mouse o con la tastiera (Invio per
   andare avanti, Esc per tornare indietro). Alla fine crea le cartelle della
   home (Documenti, Scaricati, Immagini, Musica, Video, Scrivania, Progetti).
3. **Avvii successivi**: dopo lo splash compare direttamente la scrivania.

Le scelte sono in `sys:/system/settings`; *Impostazioni → Rifai la
configurazione iniziale* la ripropone al prossimo avvio. Il CD live non
conserva nulla, quindi mostra la configurazione ogni volta.

## La scrivania

Presa a modello da macOS:

- **Barra in alto**: il logo (menu con *Info su arcOS*, *Impostazioni di
  sistema*, *Negozio*, *Monitor attività*, *Riavvia*, *Spegni*), il nome
  dell'app in primo piano, ora e data al centro, il **centro di controllo**
  (volume con un cursore, stato della rete) e il pulsante di spegnimento.
- **Widget**: saluto con l'orologio, calendario del mese, stato del sistema.
- **Dock**: File, Applicazioni, Editor di testo, Note, Calendario, Foto,
  Musica, Calcolatrice, Negozio, Impostazioni e, dopo un separatore, le app
  aperte che non ci sono già e il Cestino. Le icone **si ingrandiscono** sotto
  il puntatore, quella di un'app che parte **rimbalza**, un puntino segna le
  app aperte; un clic su un'app aperta la porta davanti (o la riapre se è
  ridotta).
- **Applicazioni** (come Launchpad, dal dock o con Ctrl+Spazio): tutte le app,
  comprese quelle installate dal Negozio, con una ricerca; si scrive e Invio
  apre la prima.
- **Notifiche** in alto a destra: un'app installata, un timer scaduto, un
  file salvato, un programma che si è chiuso per un errore. Spariscono da sole
  o con un clic.
- **Tasto destro** sulla scrivania: cambiare sfondo, impostazioni dello
  schermo, informazioni.
- **Finestre**: barra del titolo separata dal corpo, con i pulsanti riduci,
  ingrandisci e chiudi; si spostano trascinando la barra (doppio clic per
  ingrandire), molte si ridimensionano dall'angolo in basso a destra.
  **Alt+Tab** passa alla finestra successiva, **Ctrl+Q** chiude quella in
  primo piano.

### Le animazioni

Tutto ciò che si muove segue il tempo, non i fotogrammi: su un computer lento
si vedono meno fotogrammi ma l'animazione dura uguale.

| Cosa | Animazione |
|---|---|
| finestra che si apre | cresce dal 86% e compare in dissolvenza |
| finestra che si chiude | si rimpicciolisce e svanisce |
| riduci / riapri | la finestra vola nella sua icona del dock e ne esce |
| ingrandisci / ripristina | la finestra si allarga fino allo schermo e torna |
| Applicazioni | lo sfondo si sfoca e scurisce, le icone arrivano con uno zoom |
| dock | ingrandimento morbido delle icone, rimbalzo all'avvio di un'app |
| menu e centro di controllo | si aprono dall'alto in un attimo |
| notifiche | scivolano dentro da destra e poi via |
| spegnimento | lo schermo sfuma nel nero con il logo |

Una finestra in movimento viene disegnata una volta in un'immagine a parte
(con la sua trasparenza) e poi solo spostata, scalata e sfumata: l'app non
si ridisegna a ogni fotogramma.

## Le app di sistema

| App | Cosa fa |
|---|---|
| File | sfoglia i volumi: risorse a sinistra, cartelle e file a destra; doppio clic apre una cartella o il file con l'app giusta; tasto destro per aprire, rinominare (anche F2), spostare nel Cestino (Canc), creare una cartella; nel Cestino eliminare o svuotare |
| Editor di testo | file di testo con selezione, copia e incolla, scorrimento; Nuovo, Apri, Salva (Ctrl+N, Ctrl+O, Ctrl+S) con la scelta del file; alla chiusura salva da solo |
| Note | note in `sys:/home/Note`, la prima riga è il titolo, ricerca, si salvano mentre si scrive |
| Promemoria | cose da fare, spuntate con un clic (`sys:/home/Promemoria.txt`) |
| Calendario | un mese alla volta con le feste italiane (anche Pasqua e Pasquetta) e i propri eventi («9:30 Riunione»), salvati in `sys:/home/Calendario.txt` |
| Orologio | l'ora qui e in altre città, cronometro con i giri, timer che suona e avvisa |
| Calcolatrice | come quella di macOS, anche da tastiera; calcoli in virgola fissa con dodici decimali |
| Foto | le immagini di `Immagini` e gli sfondi, in miniatura e a tutto schermo |
| Musica | i file WAV di `Musica` e i suoni di sistema: riproduzione, pausa, brano precedente e successivo, barra di avanzamento, volume; tre brani di esempio |
| Anteprima | un'immagine sola, aperta da File |
| Negozio | le app di `sys:/store` per categoria, con la loro pagina; installa, apre, rimuove |
| Impostazioni | nome, tastiera, sfondo, risoluzione dello schermo, volume e suono all'avvio, configurazione iniziale |
| Monitor attività | i programmi in esecuzione, il processore e la memoria nell'ultimo minuto; un programma si può terminare |
| Info sistema | panoramica, dispositivi con i loro driver, dischi con lo spazio libero |
| Terminale | la shell di arcOS: l'unico posto dove si vedono le righe di comando |

## Il Negozio e le app installabili

Il Negozio installa programmi veri, separati dal sistema: ognuno gira in
modalità utente nel suo spazio di indirizzi e apre una finestra sulla
scrivania con le chiamate di sistema `WIN_*` (vedi
[programmi](programmi.md#programmi-con-una-finestra)). Ce ne sono sei:

| App | Categoria | |
|---|---|---|
| 2048 | Giochi | le tessere da unire, con animazioni e record |
| Campo minato | Giochi | tre livelli, bandierine con il tasto destro |
| Serpente | Giochi | sempre più veloce, record salvato |
| Sudoku | Giochi | partite sempre nuove con una sola soluzione, matita, errori in rosso |
| Tris | Giochi | contro il computer (in *Difficile* non perde mai) |
| Disegno | Creatività | matita, pennello, gomma, forme, secchiello, colori; salva in `Immagini` |

e due programmi per il Terminale, BusyBox e Lua (Linux). Ogni pacchetto è una
cartella `sys:/store/<id>` con il programma (`app`), l'icona (`icon.qoi`) e
una scheda (`info`: nome, categoria, descrizione, versione, tipo). Installare
copia questi file in `sys:/apps`; da lì le app compaiono in Applicazioni. Dal
Terminale: `install`, `install <nome>`, `uninstall <nome>`.

## Schermo e suono

Con un driver video (`bochsvga`, `vmsvga`, `virtiogpu`: le schede delle
macchine virtuali) *Impostazioni → Schermo* elenca le risoluzioni comuni
che lo schermo offre; un clic la cambia subito, e arcOS la riprende a ogni
avvio. Senza driver resta la risoluzione scelta dal firmware. Dal
Terminale: `display` elenca, `display 1920x1080` cambia.

Con una scheda audio (`hda`, `ac97`) la scrivania suona una breve melodia
quando compare (si può spegnere), il volume si regola dal centro di
controllo e dalle Impostazioni, e Musica e Orologio suonano. Dal Terminale:
`play file.wav`, `tone 440 500`, `volume 60`.

## Com'è fatta

L'interfaccia è nel kernel, in `system/kernel/src/gui`; il disegno è una
libreria condivisa con i programmi, `system/libs/arcgfx`. Niente librerie
esterne.

| File | Contenuto |
|---|---|
| `arcgfx` (`gfx.rs`, `font.rs`, `qoi.rs`) | superfici e disegno: rettangoli arrotondati antialiasati, trasparenze (anche su livelli trasparenti), sfumature, ombre, sfocatura, linee, archi, ridimensionamento; il testo dall'atlante dei glifi; le immagini QOI (lettura e scrittura). Solo aritmetica intera |
| `text.rs`, `res.rs` | i caratteri e le icone di sistema, gli sfondi |
| `splash.rs`, `setup.rs`, `desktop.rs` | avvio, configurazione guidata, scrivania con dock, Applicazioni, notifiche, centro di controllo e animazioni |
| `wm.rs` | le finestre e l'interfaccia `App` delle app |
| `window.rs` | le finestre dei programmi in modalità utente |
| `ui.rs` | componenti comuni (pulsanti, cursori, interruttori, schede), animazioni (`Anim`, curve di accelerazione), appunti, il ciclo degli eventi |
| `textarea.rs` | il testo su più righe di Editor, Note |
| `apps/` | le app di sistema |

Il disegno avviene in un buffer in memoria; sullo schermo si copia solo la
parte che cambia. Il puntatore è disegnato direttamente sullo schermo, così
muoverlo non ridisegna nulla. I pannelli "vetro smerigliato" mostrano una
copia sfocata dello sfondo calcolata una volta sola. I clic del mouse vengono
messi in coda dal driver, così un clic rapido non si perde anche se la
scrivania sta disegnando un'animazione.

La console di testo continua a esistere: con la grafica attiva è una griglia di
80x25 caratteri che la finestra Terminale disegna, e la tastiera passa alla
shell solo quando il Terminale è in primo piano.

## Le risorse

Caratteri, icone, sfondi e suoni sono generati in fase di sviluppo da script
Python (`tools/assets/`) e salvati nel repository; per ricostruire arcOS non
servono.

- `gen_fonts.py` → `system/kernel/assets/fonts.bin`: Inter e DejaVu Sans
  Mono (licenze in `system/kernel/assets/LICENSES.md`); una copia va in
  `sys:/system/fonts.bin` per i programmi.
- `gen_icons.py` → `rootfs/system/gui/icons.qoi`: oltre 70 icone, disegnate
  da zero nello stile arc (squircle con sfumatura e simbolo); con `--store`
  le icone delle app del Negozio.
- `gen_wallpapers.py` → `rootfs/system/gui/wallpapers/`: tre sfondi
  (Dolomiti, Alba, Notte) dipinti con rumore e sfumature.
- `gen_sounds.py` → `rootfs/system/sounds/avvio.wav`, il suono d'avvio, e i
  tre brani di `rootfs/home/Musica`, composti e sintetizzati dallo script.

`make assets` li rigenera (servono Python 3, Pillow e numpy).

## Provarla

`make run` apre arcOS in una finestra di QEMU. `make shots` avvia arcOS senza
schermo, lo guida con mouse e tastiera (`scripts/gui-shots.py`, attraverso
QMP) lungo `scripts/gui/tour.txt` (configurazione, ogni app, il Negozio che
installa e avvia un gioco) e salva le schermate in `build/shots/`; con
`SCREEN=1920x1080` le coordinate del giro si riferiscono a uno schermo di
quella misura.
