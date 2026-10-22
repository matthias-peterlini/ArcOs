# arcOS

arcOS è un sistema operativo per PC x86-64 scritto da zero. Non deriva da
Linux, da Unix, da DOS o da Windows, e non ne imita le interfacce: boot
loader, kernel, file system, formato dei driver, chiamate di sistema e shell
sono progettati apposta. Il codice è Rust `no_std` (più due piccoli stadi di
avvio in assembly) e non usa librerie esterne: niente crate di terze parti,
solo il compilatore e la libreria `core`/`alloc` di Rust.

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
- **Driver inclusi**: dischi IDE/ATA, AHCI (SATA), NVMe, virtio-blk; tastiera
  e mouse PS/2; rete Intel PRO/1000 (e1000).
- **ArcFS**, il file system nativo: blocchi da 4 KiB, extent, nomi
  indipendenti da maiuscole/minuscole, controllo di coerenza (`check`).
  Volumi con nome: `sys:` (il disco di sistema), `boot:` (il pacchetto di
  avvio), `ram:` (in memoria).
- **Programmi in modalità utente** (ring 3), ognuno nel proprio spazio di
  indirizzi, con chiamate di sistema basate su handle. Un programma che
  sbaglia viene fermato senza toccare il resto; Ctrl+C interrompe quello in
  primo piano. Programmi inclusi: `hello`, `info`, `calc`, `edit`, `snake`.
  Vedi [docs/programmi.md](docs/programmi.md).
- **Rete**: ARP, IPv4, ICMP (ping), UDP, client DHCP e DNS.
- **Shell** con cronologia, completamento con Tab e una quarantina di comandi
  (`help` li elenca).

L'architettura è descritta in [docs/architettura.md](docs/architettura.md).

## Compilare

Servono:

- Rust stable (la versione e i target giusti li installa `rustup` da solo,
  grazie a `rust-toolchain.toml`);
- `nasm` per i due stadi di avvio BIOS;
- `qemu-system-x86_64` per provarlo;
- OVMF (pacchetto `ovmf`) per provarlo con firmware UEFI.

```sh
make            # costruisce build/arcos.img (128 MiB, avviabile BIOS e UEFI)
make run        # lo avvia in QEMU (PC classico, disco IDE)
make run-uefi   # lo avvia in QEMU con firmware UEFI
make run-q35    # PC moderno, disco SATA (AHCI)
make run-nvme   # disco NVMe
make test       # avvio senza schermo con una sessione di comandi scritta
make test-uefi  # la stessa, avviata da UEFI
make check      # test delle librerie condivise e dello strumento host
```

`make SIZE=1G` cambia la dimensione dell'immagine, `make MEMORY=2G` la
memoria della macchina virtuale.

## Su un PC vero

`build/arcos.img` è un'immagine di disco completa: scritta su una chiavetta
USB (per esempio con `dd`, facendo molta attenzione a scegliere il
dispositivo giusto) si avvia sia in modalità BIOS/legacy sia in UEFI. Il
loader non è firmato, quindi Secure Boot va disattivato.

Da sapere prima di provarlo: arcOS non ha ancora un driver USB. Il disco di
sistema deve essere IDE, SATA (AHCI) o NVMe, e la tastiera deve essere PS/2
o una USB emulata come PS/2 dal firmware (molti PC moderni non lo fanno dopo
l'avvio UEFI). Il kernel usa un solo processore.

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
| `system/drivers/` | i driver, uno per cartella, ognuno con il suo `driver.manifest` |
| `system/apps/` | i programmi |
| `tools/arctool/` | strumento host: crea pacchetti e immagini, legge e scrive volumi ArcFS |
| `rootfs/` | file copiati nel volume di sistema |
| `scripts/` | test automatici in QEMU |

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
kbd it                 tastiera italiana
shutdown / restart
```

## Licenza

Proprietaria: vedi [LICENSE](LICENSE).
