# L'interfaccia grafica

Quando il loader trova uno schermo grafico di almeno 800x600, arcOS si avvia
in modalità grafica. Sullo schermo non compare testo: il kernel scrive i suoi
messaggi solo sulla porta seriale e nel giornale di sistema (`log` nel
Terminale li mostra).

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

- **Barra in alto**: arcOS, ora e data al centro, stato della rete e il
  pulsante di spegnimento (Riavvia, Spegni).
- **Widget**: saluto con l'orologio, calendario del mese, stato del sistema
  (memoria, da quanto è acceso, rete).
- **Dock**: File, Terminale, Editor di testo, Negozio, Impostazioni, Info
  sistema, Applicazioni, Cestino. Un puntino segna le app aperte.
- **Applicazioni** (dal dock o con Ctrl+Spazio): tutte le app, con una
  ricerca: si scrive e Invio apre la prima.
- **Tasto destro** sulla scrivania: aprire Terminale o File, cambiare sfondo,
  Impostazioni.
- **Finestre**: barra del titolo separata dal corpo, con i pulsanti riduci,
  ingrandisci e chiudi; si spostano trascinando la barra, doppio clic per
  ingrandire.

## Le app

| App | Cosa fa |
|---|---|
| File | sfoglia i volumi: risorse a sinistra, cartelle e file a destra; doppio clic apre una cartella, avvia un programma (`.app`, `.exe`, `.as`, Linux), apre un testo nell'editor o un'immagine `.qoi` |
| Terminale | la shell di arcOS in una finestra: tutti i comandi e i programmi a riga di comando (anche Linux e Windows) |
| Negozio | installa con un clic i programmi di `sys:/store` (BusyBox, Lua) |
| Impostazioni | nome, tastiera, sfondo, configurazione iniziale |
| Info sistema | processore, memoria, schermo, dispositivi, programmi |
| Editor, Calcolatrice, Snake, Note… | i programmi a riga di comando, aperti nel Terminale |

## Com'è fatta

Tutto è nel kernel, in `system/kernel/src/gui`, senza librerie esterne:

| File | Contenuto |
|---|---|
| `gfx.rs` | superfici e disegno: rettangoli arrotondati con bordi antialiasati, trasparenze, sfumature, ombre, sfocatura, ridimensionamento; solo aritmetica intera (il kernel non usa la virgola mobile) |
| `text.rs` | il testo, da un atlante di glifi antialiasati (`assets/fonts.bin`) |
| `qoi.rs` | il decodificatore delle immagini QOI |
| `res.rs` | icone e sfondi da `sys:/system/gui` |
| `splash.rs`, `setup.rs`, `desktop.rs` | avvio, configurazione guidata, scrivania |
| `wm.rs`, `apps/` | le finestre e le app |
| `cursor.rs`, `ui.rs` | il puntatore, i componenti comuni e il ciclo degli eventi |

Il disegno avviene in un buffer in memoria; sullo schermo si copia solo la
parte che cambia. Il puntatore è disegnato direttamente sullo schermo, così
muoverlo non ridisegna nulla. I pannelli "vetro smerigliato" mostrano una copia
sfocata dello sfondo calcolata una volta sola.

La console di testo continua a esistere: con la grafica attiva è una griglia di
80x25 caratteri che la finestra Terminale disegna, e la tastiera passa alla
shell solo quando il Terminale è in primo piano. In caso di errore fatale il
kernel riprende lo schermo e mostra il messaggio.

## Le risorse

Caratteri, icone e sfondi sono generati in fase di sviluppo da script Python
(`tools/assets/`) e salvati nel repository; per ricostruire arcOS non servono.

- `gen_fonts.py` → `system/kernel/assets/fonts.bin`: Inter e DejaVu Sans
  Mono (licenze in `system/kernel/assets/LICENSES.md`).
- `gen_icons.py` → `rootfs/system/gui/icons.qoi`: oltre 60 icone, disegnate
  da zero nello stile arc (squircle con sfumatura e simbolo).
- `gen_wallpapers.py` → `rootfs/system/gui/wallpapers/`: tre sfondi
  (Dolomiti, Alba, Notte) dipinti con rumore e sfumature.

`make assets` li rigenera (servono Python 3, Pillow e numpy).

## Provarla

`make run` apre arcOS in una finestra di QEMU. `make shots` avvia arcOS senza
schermo, lo guida con mouse e tastiera (`scripts/gui-shots.py`, attraverso
QMP) e salva le schermate in `build/shots/`.
