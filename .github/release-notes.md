arcOS: un sistema operativo per PC x86-64 scritto da zero, con il linguaggio ArcScript.

![La scrivania di arcOS](https://raw.githubusercontent.com/matthias-peterlini/ArcOs/main/docs/screenshots/scrivania.jpg)

## Novità della 0.4

- **Audio**: driver per Intel HD Audio (quasi ogni PC dal 2005) e AC'97. Una breve melodia all'avvio della scrivania (si può spegnere), volume nelle Impostazioni, `play file.wav`, `tone` e `volume` nel Terminale.
- **Risoluzione dello schermo**: driver per le schede video di QEMU, Bochs, VirtualBox (VBoxVGA e VMSVGA), VMware e virtio-gpu. *Impostazioni → Schermo* cambia la risoluzione al volo (fino a 2560×1600) e arcOS la ricorda; nel Terminale `display 1920x1080`.
- **Lettori di schede SD**: schede SD, SDHC, SDXC e MMC nei lettori standard dei portatili, inserite ed estratte a sistema avviato.
- **Corretto**: il puntatore del mouse si muoveva al contrario in verticale.
- Il controllo dei volumi (`check`) è molto più veloce sui dischi grandi.

Ancora senza driver: Wi-Fi, schede video vere (Intel, AMD, nVidia: si usa il framebuffer del firmware), schede di rete a 2,5 Gbit, batteria dei portatili. Lo stato di ogni driver è in [docs/driver.md](https://github.com/matthias-peterlini/ArcOs/blob/main/docs/driver.md#hardware-supportato).

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
