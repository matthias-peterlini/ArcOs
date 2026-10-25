<div align="center">

# arcOS

**Un sistema operativo per PC x86-64 scritto da zero.**

Boot loader, kernel, file system, driver, interfaccia grafica e linguaggio
di programmazione: tutto progettato apposta, in Rust, senza librerie esterne.

[**Scarica la versione 0.4.0**](https://github.com/matthias-peterlini/ArcOs/releases/latest) ·
[Documentazione](docs/) ·
[Architettura](docs/architettura.md)

![La scrivania di arcOS](docs/screenshots/scrivania.jpg)

</div>

## In breve

- **Si avvia ovunque**: la stessa immagine parte da BIOS e da UEFI, da
  chiavetta USB, da disco o come CD live.
- **Un avvio pulito**: niente righe di testo, solo il logo e una barra di
  caricamento. Al primo avvio una configurazione guidata (nome, tastiera,
  sfondo), poi direttamente la scrivania.
- **Una scrivania moderna**: pannelli in vetro smerigliato, dock, widget,
  lanciatore con ricerca, finestre; icone, sfondi e suoni disegnati per
  arcOS.
- **I driver si installano da soli**: dischi SATA e NVMe, USB 1/2/3,
  schede di rete Intel e Realtek, lettori di schede SD, audio HD e AC'97,
  schede video delle macchine virtuali con cambio di risoluzione.
- **Esegue programmi Linux e Windows** x86-64 senza modificarli, accanto
  ai suoi.
- **Ha il suo linguaggio**, ArcScript, con compilatore e macchina virtuale
  che girano su arcOS.
- **Scritto da zero**: niente codice di Linux, Unix, DOS o Windows, nessuna
  crate di terze parti. Solo il compilatore Rust e `core`/`alloc`.

## Com'è

<table>
<tr>
<td width="50%"><img src="docs/screenshots/avvio.jpg" alt="Avvio"><br><b>Avvio</b>: solo il logo e una barra di caricamento.</td>
<td width="50%"><img src="docs/screenshots/benvenuto.jpg" alt="Primo avvio"><br><b>Primo avvio</b>: la configurazione guidata parte con INIZIA.</td>
</tr>
<tr>
<td><img src="docs/screenshots/sfondo.jpg" alt="Scelta dello sfondo"><br><b>Configurazione</b>: nome, tastiera, sfondo, in un minuto.</td>
<td><img src="docs/screenshots/finestre.jpg" alt="File e Terminale"><br><b>File e Terminale</b>: le cartelle e la shell, in finestre.</td>
</tr>
<tr>
<td><img src="docs/screenshots/impostazioni.jpg" alt="Impostazioni"><br><b>Impostazioni</b>: risoluzione, volume, tastiera, sfondo.</td>
<td><img src="docs/screenshots/applicazioni.jpg" alt="Applicazioni"><br><b>Applicazioni</b>: tutte le app, con ricerca (Ctrl+Spazio).</td>
</tr>
</table>

## Provarlo

Nelle [release](https://github.com/matthias-peterlini/ArcOs/releases)
ci sono due file:

| File | Cosa farne |
|---|---|
| `arcos.iso` | **CD live**: si avvia in una macchina virtuale (VirtualBox, VMware, QEMU) o da un CD, con BIOS o UEFI. Il sistema gira in memoria e le modifiche si perdono allo spegnimento. |
| `arcos-img.zip` | **Immagine di disco** da scrivere su una chiavetta USB (balenaEtcher, Rufus in modalità DD, `dd`) per avviare un PC vero. Secure Boot va disattivato. |

Il modo più veloce, con QEMU:

```sh
qemu-system-x86_64 -m 512M -cdrom arcos.iso -device ich9-intel-hda -device hda-output
```

In VirtualBox basta una macchina "Other/Unknown (64-bit)" con 512 MiB di
memoria e `arcos.iso` nel lettore CD.

## Cosa c'è dentro

| Parte | Cosa fa | Dettagli |
|---|---|---|
| **ArcBoot** | loader per BIOS (MBR + secondo stadio in assembly) e per UEFI (un'applicazione EFI in Rust); entrambi passano al kernel la stessa `BootInfo` | [architettura](docs/architettura.md#avvio) |
| **Kernel** | 64 bit, memoria virtuale a 4 livelli, scheduler preemptivo, thread, code di attesa, MSI/MSI-X, ACPI per spegnere e riavviare | [architettura](docs/architettura.md#kernel) |
| **Driver** | pacchetti `.adp` indipendenti dal kernel: il kernel confronta i manifesti con l'hardware e carica il driver giusto da solo, all'avvio e a sistema avviato | [driver](docs/driver.md) |
| **ArcFS** | il file system nativo: blocchi da 4 KiB, extent, controllo di coerenza; volumi con nome (`sys:`, `ram:`, `chiavetta:`) | [architettura](docs/architettura.md#file-system-arcfs) |
| **Interfaccia** | splash, configurazione guidata, scrivania, finestre e app: File, Terminale, Negozio, Impostazioni, Info sistema | [gui](docs/gui.md) |
| **Programmi** | in modalità utente, ognuno nel suo spazio di indirizzi; `hello`, `info`, `calc`, `edit`, `snake` | [programmi](docs/programmi.md) |
| **Compatibilità** | eseguibili ELF statici Linux e `.exe` Windows x64 da console; il Negozio installa BusyBox e Lua | [compatibilità](docs/compat.md) |
| **ArcScript** | il linguaggio di arcOS: `arcc` compila ed esegue `.as` e pacchetti `.arc` | [ArcScript](docs/arcscript.md) |
| **Rete** | ARP, IPv4, ICMP, UDP, DHCP, DNS | |
| **Shell** | una quarantina di comandi, cronologia, completamento con Tab | [primi passi](#primi-passi-nel-terminale) |

### Hardware

| Tipo | Supportato |
|---|---|
| Dischi | IDE, SATA (AHCI), NVMe, virtio |
| USB | controller USB 3, 2 e 1.1, hub, tastiere, mouse, tavolette, chiavette e dischi (anche come disco di sistema), collegamento a caldo |
| Schede SD | lettori standard (SD Host Controller): schede SD, SDHC, SDXC, MMC |
| Rete | Intel PRO/1000 (e1000, 82574, I217–I219), Intel 82576, I210/I211, I350 e I225/I226 (2,5 Gbit), Realtek RTL8139, RTL8169/8111/8168 e RTL8125 (2,5 Gbit), AMD PCnet, virtio |
| Audio | Intel HD Audio (quasi ogni PC dal 2005), AC'97 |
| Video | il framebuffer del firmware su qualunque scheda; cambio di risoluzione con le schede di QEMU, VirtualBox (VBoxVGA, VMSVGA), VMware e virtio-gpu |
| Input | tastiera e mouse PS/2 e USB, pulsante di accensione (ACPI) |

Mancano ancora il Wi-Fi, i driver per le schede video vere (Intel, AMD,
nVidia) e la batteria dei portatili. Lo stato
di ogni driver, provato o no, è in [docs/driver.md](docs/driver.md#hardware-supportato).

## Compilare

Servono Rust stable (`rustup` installa da solo la versione e i target
giusti, grazie a `rust-toolchain.toml`), `nasm`, `qemu-system-x86_64` e,
per provare l'avvio UEFI, OVMF.

```sh
make            # l'immagine di disco build/arcos.img (BIOS e UEFI)
make iso        # il CD live build/arcos.iso
make run        # avvia arcOS in QEMU (anche run-uefi, run-q35, run-nvme, run-iso)
```

I test girano tutti in QEMU, senza schermo:

```sh
make test           # una sessione di comandi scritta (anche test-uefi, test-iso)
make test-drivers   # un avvio per ogni disco, scheda di rete, controller USB e lettore SD
make test-audio     # suoni su ogni scheda audio emulata, registrati e controllati
make test-display   # cambio di risoluzione su ogni scheda video emulata
make test-acpi      # il pulsante di accensione spegne arcOS
make test-compat    # un programma Linux e uno Windows
make test-arcscript # programmi ArcScript
make shots          # usa l'interfaccia con mouse e tastiera e salva le schermate
make check          # test delle librerie e dello strumento host
```

`make SIZE=1G` cambia la dimensione dell'immagine, `make MEMORY=2G` la
memoria della macchina virtuale.

## Su un PC vero

`build/arcos.img` (o `arcos-img.zip` delle release) scritta su una
chiavetta USB si avvia sia in modalità BIOS/legacy sia UEFI; il loader non
è firmato, quindi Secure Boot va disattivato. Il disco di sistema può
essere la chiavetta stessa.

Da sapere prima di provarlo:

- il kernel usa un solo processore;
- lo schermo resta nella risoluzione scelta dal firmware (le schede video
  vere non hanno ancora un driver);
- molti driver sono scritti dalla documentazione dei chip e provati in
  QEMU, non ancora su hardware vero.

## Primi passi nel Terminale

```
help                   tutti i comandi
dev / drv              dispositivi trovati e driver installati
vol / disks            volumi montati, dischi e partizioni
ls sys:/home           i tuoi file
display 1920x1080      cambia risoluzione (con un driver video)
play sys:/system/sounds/avvio.wav
volume 60
net / ping 10.0.2.2    rete
install busybox        un programma dal negozio
cd sys:/home/arcscript
hello.as               un programma ArcScript
shutdown / restart
```

## Struttura del repository

| Percorso | Contenuto |
|---|---|
| `boot/bios/`, `boot/uefi/` | ArcBoot per BIOS e per UEFI |
| `system/kernel/` | il kernel, con l'interfaccia grafica (`src/gui`) |
| `system/drivers/` | i driver, uno per cartella, ognuno con il suo `driver.manifest` |
| `system/apps/` | i programmi, tra cui `winrun` (programmi Windows) e `arcc` (ArcScript) |
| `system/libs/` | le librerie: ABI dei driver (`arcdk`, `arcdrv`), programmi (`arcrt`), ArcFS, USB, virtio, ArcScript |
| `tools/arctool/` | strumento host: crea pacchetti, immagini e CD, legge e scrive volumi ArcFS |
| `tools/assets/` | i generatori di caratteri, icone, sfondi e suoni |
| `tools/qemu/` | modelli per QEMU di dispositivi che QEMU non ha (Realtek RTL8169, RTL8168, RTL8125) |
| `rootfs/` | i file del volume di sistema |
| `scripts/` | i test automatici in QEMU |
| `docs/` | la documentazione |

## Licenza

Proprietaria: vedi [LICENSE](LICENSE). I caratteri Inter e DejaVu Sans
Mono hanno le loro licenze libere, in `system/kernel/assets/LICENSES.md`.
