PHANTOM — ETAPA 2C-4 FIX 1
UNIFICAÇÃO IMAGE-RS 0.25.10

Problema:
- phantom-image exigia image = "=0.25.10"
- phantom-browser exigia image = "=0.25.8"
- Cargo não pode resolver duas versões exatas conflitantes do mesmo pacote
  dentro desse grafo de dependências.

Correção:
- phantom-browser -> image = "=0.25.10"
- phantom-image   -> image = "=0.25.10"
- mantém default-features = false
- browser continua habilitando apenas "png"
- phantom-image habilita "png", "jpeg", "gif", "webp"

Motivo técnico:
image 0.25.10 adiciona metadata::LoopCount e AnimationDecoder::loop_count,
APIs usadas pela 2C-4 para GIF/WebP animados.

Aplicar sobre a 2C-4 atual.

Depois execute:

cargo update -p image --precise 0.25.10
cargo fmt --all
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p phantom-image --test animation_decode
cargo test -p phantom-image --test raster_decode

Se todos passarem:

taskkill /F /IM phantom-browser.exe
cargo build --release -p phantom-browser
.\target\release\phantom-browser.exe
