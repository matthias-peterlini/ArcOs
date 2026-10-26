arcOS: un sistema operativo per PC x86-64 scritto da zero, con il linguaggio ArcScript.

![La scrivania di arcOS](https://raw.githubusercontent.com/matthias-peterlini/ArcOs/main/docs/screenshots/scrivania.jpg)

## Novità della 0.5

- **Una scrivania alla macOS**: il dock ingrandisce le icone sotto il puntatore e fa rimbalzare quella dell'app che parte; finestre animate (si aprono, si chiudono, volano nel dock, si ingrandiscono); Launchpad con sfocatura e zoom; centro di controllo con il volume; notifiche; Alt+Tab e Ctrl+Q; finestre ridimensionabili.
- **Niente righe di comando fuori dal Terminale**: ogni icona apre un'app grafica, e un errore grave mostra una schermata semplice.
- **Nuove app**: Editor di testo, Note, Promemoria, Calendario (con le feste italiane), Orologio (città, cronometro, timer), Calcolatrice, Foto, Musica (con tre brani di esempio), Monitor attività; File con il menu del tasto destro (rinomina, Cestino, nuova cartella).
- **Il Negozio installa app vere**: 2048, Campo minato, Serpente, Sudoku, Tris e Disegno, programmi separati con le loro finestre; categorie, pagina di ogni app, installa, apri, rimuovi. I programmi possono aprire finestre con la nuova libreria `arcui`.
- **Corretto**: il mouse USB poteva smettere di funzionare dopo qualche secondo; i clic rapidi durante un'animazione potevano andare persi.

![Il Negozio](https://raw.githubusercontent.com/matthias-peterlini/ArcOs/main/docs/screenshots/negozio.jpg)

Ancora senza driver: Wi-Fi, schede video vere (Intel, AMD, nVidia: si usa il framebuffer del firmware), batteria dei portatili. Lo stato di ogni driver è in [docs/driver.md](https://github.com/matthias-peterlini/ArcOs/blob/main/docs/driver.md#hardware-supportato).

## File

| File | Cosa farne |
|---|---|
| `arcos.iso` | **CD live**: si avvia in una macchina virtuale (VirtualBox, VMware, QEMU, Hyper-V di prima generazione) o da un CD, con BIOS o UEFI, senza disco rigido. Il sistema gira in memoria: le modifiche si perdono allo spegnimento. |
| `arcos-img.zip` | **Immagine di disco** (128 MiB, BIOS e UEFI): scompattata, si scrive su una chiavetta USB (balenaEtcher, Rufus in modalità DD, `dd`) per avviare un PC vero, con le modifiche che restano. Secure Boot va disattivato. |
| `SHA256SUMS` | le impronte dei due file |

## Provarlo

- **QEMU**: `qemu-system-x86_64 -m 512M -cdrom arcos.iso -device ich9-intel-hda -device hda-output`
- **VirtualBox**: nuova macchina "Other/Unknown (64-bit)", 512 MiB di memoria, nessun disco, `arcos.iso` nel lettore CD.

Al primo avvio arcOS chiede nome, tastiera e sfondo, poi apre la scrivania. Il Terminale (nel dock) ha la shell: `help` elenca i comandi. Documentazione nel [README](https://github.com/matthias-peterlini/ArcOs#readme).
