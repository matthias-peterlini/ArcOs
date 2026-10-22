# I driver di arcOS

In arcOS un driver è un **pacchetto**: un file `.adp` che contiene un
manifesto di testo e il modulo compilato. Il kernel legge i manifesti, li
confronta con l'hardware e carica da solo il driver giusto. Installarne uno
vuol dire solo mettere il pacchetto dove il kernel lo trova.

## Come vengono scelti e installati

All'avvio il kernel enumera i dispositivi: il bus PCI e i dispositivi di
piattaforma (il controller PS/2 con tastiera e mouse). Ogni dispositivo
diventa un `DeviceInfo` con id, bus, vendor/device, classe, IRQ, risorse
(BAR) e, per quelli di piattaforma, un nome come `ps2-keyboard`.

I pacchetti stanno in due posti:

| Dove | Cosa |
|---|---|
| `boot:/drivers` | il **pacchetto di avvio** (boot pack): i driver che servono prima che il disco di sistema sia montato. Il loader lo carica in memoria insieme al kernel |
| `sys:/system/drivers` | il **magazzino** (driver store): tutti i driver disponibili per questa installazione |

Per ogni dispositivo senza driver, il gestore:

1. valuta le regole `match` di tutti i manifesti. Una regola più specifica
   vale di più: un id PCI esatto o un nome di piattaforma vale 100,
   `pci-class` classe:sottoclasse:interfaccia 60, classe:sottoclasse 40, la
   sola classe 20;
2. a parità di punteggio sceglie la versione più recente; a parità di
   versione preferisce il magazzino al pacchetto di avvio;
3. carica il modulo (una volta sola, anche se serve a più dispositivi) e
   chiama `probe` sul dispositivo. Se `probe` rifiuta, prova il candidato
   successivo.

Questo succede in tre momenti:

- **all'avvio**: prima con i driver del pacchetto di avvio (così si monta
  `sys:`), poi con tutto il magazzino;
- **dopo ogni scansione** dei dispositivi (`dev scan`);
- **quando arriva un pacchetto nuovo**. Il thread `drvmon` controlla
  `sys:/inbox` ogni due secondi. Un pacchetto lasciato lì viene verificato
  (formato, CRC, manifesto, versione dell'ABI), spostato nel magazzino e
  installato. Un pacchetto non valido viene rinominato in
  `<nome>.adp.rejected`, con il motivo nel giornale (`log`).

Se arriva una versione più nuova di un driver già in esecuzione, la vecchia
viene scaricata e sostituita subito, se il driver lo permette (cioè se ha
`remove`). Altrimenti la sostituzione avviene al riavvio.

### Comandi

```
dev                  dispositivi, con il driver di ognuno
dev 5                dettagli del dispositivo 5
dev scan             cerca hardware nuovo e installa i driver mancanti
drv                  tutti i driver: dove si trovano e se sono in uso
drv info <nome>      manifesto, file, dispositivi gestiti
drv add <file.adp>   installa un pacchetto (come l'inbox, ma subito)
drv remove <nome>    toglie il driver dal magazzino e, se può, lo scarica
drv reload <nome>    scarica e ricarica un driver
drv inbox            controlla l'inbox adesso
```

Dal computer di sviluppo si può mettere un pacchetto nell'inbox di
un'immagine senza avviarla:

```sh
tools/target/release/arctool fs build/arcos.img put mio.adp /inbox/
```

## Il manifesto

Righe `chiave = valore`. `#` inizia un commento. Le chiavi sconosciute
vengono ignorate, così un manifesto più nuovo si carica anche su un kernel
più vecchio.

```text
# Intel PRO/1000 Ethernet.
name        = e1000                 # obbligatorio: nome unico del driver
version     = 1.0.0                 # obbligatorio: major.minor.patch
class       = network               # storage, input, network, display, other
description = Intel PRO/1000 Ethernet
author      = arcOS
boot        = no                    # yes: va nel pacchetto di avvio
match       = pci 8086:100e         # vendor:device (esadecimale)
match       = pci 8086:10d3
```

Le regole `match` possibili:

| Regola | Esempio | Significato |
|---|---|---|
| `pci VVVV:DDDD` | `pci 1af4:1001` | un modello preciso |
| `pci-class CC[:SS[:PP]]` | `pci-class 01:06:01` | una classe PCI (qui: SATA AHCI) |
| `platform <id>` | `platform ps2-mouse` | un dispositivo di piattaforma |

Un manifesto senza regole è valido, ma il driver non verrà mai scelto
automaticamente (`arctool pack` lo segnala).

## Scrivere un driver

Un driver è un crate Rust `no_std` nella cartella `system/drivers/<nome>/`,
che usa la libreria `arcdrv`. Il `Makefile` trova da solo ogni cartella
lì dentro, la compila, ne fa un pacchetto e lo mette nel magazzino
dell'immagine (e nel pacchetto di avvio se il manifesto dice `boot = yes`).

```
system/drivers/esempio/
├── Cargo.toml
├── build.rs
├── driver.manifest
└── src/main.rs
```

`Cargo.toml`:

```toml
[package]
name = "esempio"
description = "arcOS driver: esempio"
version = "1.0.0"
edition.workspace = true
publish.workspace = true
authors.workspace = true

[[bin]]
name = "esempio"
path = "src/main.rs"
test = false
bench = false

[dependencies]
arcdrv.workspace = true

[lints]
workspace = true
```

`build.rs` (uguale per tutti i driver: un'immagine indipendente dalla
posizione, con il punto d'ingresso `arc_driver_entry`):

```rust
fn main() {
    println!("cargo:rustc-link-arg-bins=-e");
    println!("cargo:rustc-link-arg-bins=arc_driver_entry");
    println!("cargo:rustc-link-arg-bins=--strip-all");
    println!("cargo:rustc-link-arg-bins=-z");
    println!("cargo:rustc-link-arg-bins=max-page-size=4096");
}
```

`src/main.rs`:

```rust
#![no_std]
#![no_main]

use arcdrv::{DeviceInfo, IrqLock, Mmio, Pci, Status, E_NODEV, OK};
use core::ffi::c_void;

arcdrv::driver! {
    name: "esempio",
    version: "1.0.0",
    probe: probe,
    remove: remove,      // facoltativo: senza, il driver non si può scaricare
}

/// I registri del dispositivo, finché il driver lo gestisce.
static REGS: IrqLock<Option<Mmio>> = IrqLock::new(None);

/// Chiamata per ogni dispositivo che il manifesto ha scelto.
fn probe(dev: &DeviceInfo) -> Status {
    let bar = dev.resources[0];
    let Some(regs) = Mmio::map(bar.base, bar.size as usize) else {
        return E_NODEV; // il kernel prova il candidato successivo
    };
    Pci(dev.id).enable(false, true, true); // memoria e bus master
    let version = regs.read32(0);
    *REGS.lock() = Some(regs);
    let status = arcdrv::irq_attach(dev.id, dev.irq, irq, core::ptr::null_mut());
    if status != OK {
        return status;
    }
    arcdrv::info!("versione {version:#x}, IRQ {}", dev.irq);
    OK
}

unsafe extern "C" fn irq(_ctx: *mut c_void) {
    if let Some(regs) = REGS.lock().as_ref() {
        regs.write32(4, 1); // per esempio: conferma l'interruzione
    }
}

fn remove(device: u32) {
    arcdrv::irq_detach(device, 0);
    REGS.lock().take();
}
```

I driver in `system/drivers/` sono esempi completi, dal più semplice,
`ps2kbd`, a `nvme`.

### Cosa offre il kernel

Un modulo non vede il kernel: riceve solo una tabella di funzioni
(`KernelApi`, in `system/libs/arcdk/src/abi.rs`), che `arcdrv` avvolge in
funzioni sicure:

| Servizio | In `arcdrv` |
|---|---|
| giornale | `info!`, `warn!`, `error!`, `debug!` (il prefisso col nome del driver è automatico) |
| memoria | allocatore globale: `alloc::vec::Vec`, `Box`, `String` funzionano |
| registri | `Mmio` (memoria del dispositivo, mappata non in cache), `port::inb/outb/...` |
| DMA | `Dma`: memoria fisicamente contigua con il suo indirizzo fisico |
| PCI | `Pci`: spazio di configurazione, bus master |
| interruzioni | `irq_attach`, `irq_detach` |
| tempo e thread | `time_ms`, `sleep_ms`, `yield_now`, `wait_until`, `spawn` |
| sincronizzazione | `IrqLock` (spinlock con interruzioni spente), `Mutex` |
| dischi | `block_register` con un `BlockDesc` (lettura, scrittura, flush) |
| input | `input_event` (tasti come usage HID, movimenti, pulsanti) |
| rete | `net_register` con un `NetDesc`, `net_receive` per i frame in arrivo |

Il driver dichiara la versione dell'ABI per cui è compilato
(`ABI_VERSION`). Il kernel rifiuta i pacchetti di un'ABI diversa invece di
caricarli e rischiare un errore.

### Il pacchetto

Con `make` i pacchetti si creano da soli in `build/drivers/`. A mano:

```sh
cd system && cargo build --release -p esempio
tools/target/release/arctool pack system/drivers/esempio/driver.manifest \
    system/target/x86_64-unknown-none/release/esempio esempio.adp
tools/target/release/arctool inspect esempio.adp
```

`arctool pack` controlla il manifesto e che il modulo sia un'immagine
caricabile: ELF64, indipendente dalla posizione, con le sole rilocazioni
relative e il punto d'ingresso giusto.

Formato `.adp` (interi little-endian):

```text
offset  dim.  campo
     0     8  "ARCDRVPK"
     8     4  versione del formato (1)
    12     4  versione dell'ABI dei driver
    16     4  posizione del manifesto    20  4  lunghezza
    24     4  posizione del modulo       28  4  lunghezza
    32     4  CRC-32 di tutto ciò che segue l'intestazione
    36     4  riservato
    40        manifesto (UTF-8), modulo (ELF64)
```
