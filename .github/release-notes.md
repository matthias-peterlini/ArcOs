arcOS: un sistema operativo per PC x86-64 scritto da zero, con il linguaggio ArcScript.

## Novità

- **Interfaccia grafica**: avvio pulito con solo logo e barra di caricamento, configurazione guidata al primo avvio (pulsante **INIZIA**, nome, tastiera, sfondo), poi la scrivania: barra in alto, widget (orologio, calendario, sistema), dock, lanciatore con ricerca (Ctrl+Spazio) e finestre.
- **App**: File, Terminale (la shell), Negozio (installa BusyBox e Lua con un clic), Impostazioni, Info sistema.
- Icone, sfondi e caratteri disegnati per arcOS.
- Restano i programmi **Linux** (ELF statici) e **Windows** (`.exe` x64 da console).

Dettagli in [docs/gui.md](https://github.com/matthias-peterlini/ArcOs/blob/main/docs/gui.md) e [docs/compat.md](https://github.com/matthias-peterlini/ArcOs/blob/main/docs/compat.md).

## File

| File | Cosa farne |
|---|---|
| `arcos.iso` | **CD live**: si avvia in una macchina virtuale (VirtualBox, VMware, QEMU, Hyper-V di prima generazione) o da un CD, con BIOS o UEFI, senza disco rigido. Il sistema gira in memoria: le modifiche si perdono allo spegnimento. |
| `arcos-img.zip` | **Immagine di disco** (128 MiB, BIOS e UEFI): scompattata, si scrive su una chiavetta USB (balenaEtcher, Rufus in modalità DD, `dd`) per avviare un PC vero, con le modifiche che restano. Secure Boot va disattivato. |
| `SHA256SUMS` | le impronte dei due file |

## Provarlo

- **QEMU**: `qemu-system-x86_64 -m 512M -cdrom arcos.iso`
- **VirtualBox**: nuova macchina "Other/Unknown (64-bit)", 512 MiB di memoria, nessun disco, `arcos.iso` nel lettore CD.

Al primo avvio arcOS chiede nome, tastiera e sfondo, poi apre la scrivania. Il Terminale (nel dock) ha la shell: `help` elenca i comandi. Documentazione nel [README](https://github.com/matthias-peterlini/ArcOs#readme).
