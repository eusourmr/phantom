PHANTOM — ETAPA 2C-2
IMAGE FETCH + DECODE + RASTER PAINT

Aplicar sobre a 2C-1 + fix1 validados.

ARQUIVOS COMPLETOS:
- crates/phantom-net/Cargo.toml
- crates/phantom-net/src/lib.rs
- crates/phantom-image/Cargo.toml
- crates/phantom-image/src/lib.rs
- crates/phantom-image/tests/raster_decode.rs
- crates/phantom-image/tests/fixtures/rgba-2x1.png
- crates/phantom-image/tests/fixtures/rgb-2x1.jpg
- crates/phantom-browser/Cargo.toml
- crates/phantom-browser/src/main.rs
- docs/ENGINE-2C2-IMAGE-FETCH-DECODE-RASTER.md
- docs/STANDARDS-MAP-IMAGE-LOADING.md

NOVO:
- fetch binário bounded no phantom-net
- PNG raster decode real
- JPEG raster decode real
- ImageDecoder continua isolando codec
- resolução de src relativo contra URL final do documento
- worker de imagens fora da thread da UI
- carregamento progressivo
- metadata -> relayout sem reparse/re-cascade
- RGBA8 -> textura egui
- PaintCommand::Image revela raster real
- placeholder continua para falha/formato ainda não suportado
- limite de 64 imagens por documento nesta fase
- limite aproximado de 256 MiB de raster/textura por tab
- fixtures e integration tests de PNG/JPEG

LIMITES EXPLÍCITOS:
- GIF: probe sim, decode não
- WebP/AVIF: ainda não
- srcset/sizes/picture: ainda não
- <base>: ainda não integrado ao resource resolver
- CSS background-image: ainda não
- object-fit/object-position: ainda não
- cache compartilhado: ainda não

GATES:

cargo fmt --all
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p phantom-image --test raster_decode

Somente se TODOS passarem:

taskkill /F /IM phantom-browser.exe
cargo build --release -p phantom-browser
.\target\release\phantom-browser.exe

TESTE VISUAL:
1. abrir uma página com PNG/JPEG usando src normal;
2. confirmar placeholder primeiro;
3. confirmar substituição progressiva pela imagem real;
4. testar página pública como globo.com;
5. observar no status quantidade exibida/falhas.

PRÓXIMO MARCO RECOMENDADO:
2C-3 — Responsive Images + Object Sizing + Resource Cache v1.
