# Release e aggiornamenti

La versione in `src-tauri/Cargo.toml` avvia una release quando aumenta su `main`. La pipeline crea e firma i pacchetti Ubuntu (`.deb`), Fedora (`.rpm`), Linux (`.AppImage`) e Windows (`.exe`), poi pubblica i file `.sig`, `SHA256SUMS.txt` e `latest.json` nella GitHub Release. Le versioni preliminari sono pubblicate come prerelease e non sostituiscono il canale stabile `releases/latest`.

Legio controlla gli aggiornamenti all'avvio e dalle impostazioni. Il manifest distingue il formato installato. Tauri verifica la firma prima di installare. Su Ubuntu e Fedora l'installazione del pacchetto può richiedere l'autorizzazione amministrativa. Su Windows viene avviato l'installer NSIS. Dopo l'installazione, Legio si riavvia.

Il pacchetto Arch `legio-launcher-bin` usa l'AppImage pubblicata su GitHub. La pipeline aggiorna il PKGBUILD e `.SRCINFO` su AUR per ogni release stabile. Chi usa il pacchetto AUR aggiorna tramite il proprio gestore di pacchetti; Legio mostra la nuova versione senza sovrascrivere i file gestiti da pacman.

## Credenziali

- `TAURI_SIGNING_PRIVATE_KEY` contiene la chiave privata Tauri per firmare gli installer. La chiave pubblica è in `src-tauri/tauri.conf.json`. Conservare una copia privata sicura: perderla impedisce di aggiornare le installazioni esistenti.
- `AUR_SSH_PRIVATE_KEY` contiene una chiave SSH dedicata. La relativa chiave pubblica va aggiunta al profilo AUR che pubblica `legio-launcher-bin`.

Le due chiavi sono nei secret GitHub del repository. Le copie locali sono in `~/.config/legio-release/` e non vanno committate. Una release stabile fallisce nel job AUR finché la chiave pubblica SSH non è registrata nel profilo AUR.
