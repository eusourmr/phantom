PHANTOM — ETAPA 2C-3
RESPONSIVE IMAGES + OBJECT SIZING + RESOURCE CACHE V1

BASE OBRIGATÓRIA:
2C-2 homologada + FIX 1 + FIX 2.

ARQUIVOS COMPLETOS MODIFICADOS:
- crates/phantom-css/src/lib.rs
- crates/phantom-engine/src/lib.rs
- crates/phantom-engine/tests/responsive_images.rs
- crates/phantom-image/Cargo.toml
- crates/phantom-image/src/lib.rs
- crates/phantom-image/tests/raster_decode.rs
- crates/phantom-image/tests/fixtures/rgba-2x1.webp
- crates/phantom-net/src/lib.rs
- crates/phantom-paint/src/lib.rs
- crates/phantom-browser/src/main.rs
- docs/ENGINE-2C3-RESPONSIVE-IMAGES-OBJECT-CACHE.md
- docs/STANDARDS-MAP-RESPONSIVE-IMAGES.md

ENTRA NESTA VERSÃO:
- srcset 1x/2x etc.
- srcset 400w/800w etc.
- sizes: px/vw + media simples min-width/max-width
- picture/source
- seleção por viewport e DPR
- object-fit: fill/contain/cover/none/scale-down
- object-position: keywords + porcentagens
- WebP estático
- deduplicação por URL no documento
- cache de raster bounded por documento/tab
- reaproveitamento de uma textura entre múltiplos ImageResourceId
- testes executáveis de responsive images
- fixture real WebP 2x1

NÃO É DECLARADO COMO COMPLETO:
- algoritmo WHATWG integral de srcset/sizes
- media queries complexas
- sizes=auto completo
- HTTP cache / Cache-Control / ETag / Vary
- GIF animado
- WebP animado
- AVIF
- data:/blob:
- CSS background-image

GATES:

cargo fmt --all
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p phantom-engine --test responsive_images
cargo test -p phantom-image --test raster_decode

SE TODOS PASSAREM:

taskkill /F /IM phantom-browser.exe
cargo build --release -p phantom-browser
.\target\release\phantom-browser.exe

NÃO RELAXAR WARNINGS OU CLIPPY PARA FAZER O BUILD PASSAR.
