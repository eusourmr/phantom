PHANTOM — ETAPA 2B-10
FLEXBOX FINAL CORE + AUTO MARGINS + EXECUTABLE WPT SLICE

Aplicar sobre a 2B-9 validada.

ARQUIVOS COMPLETOS:
- crates/phantom-css/src/lib.rs
- crates/phantom-layout/Cargo.toml
- crates/phantom-layout/src/lib.rs
- crates/phantom-layout/tests/wpt_flexbox_slice.rs
- crates/phantom-engine/src/lib.rs
- docs/ENGINE-2B10-FLEXBOX-FINAL-CORE.md
- docs/ENGINEERING-STANDARDS-DOCTRINE.md
- docs/WPT-FLEXBOX-EXECUTABLE-SLICE.md

NOVO:
- AutoEdges tipado
- margin:auto preservado semanticamente no ComputedStyle
- margin:auto no eixo principal de Flexbox
- múltiplas margens auto dividem o espaço livre
- margin:auto no cross-axis
- auto margin tem precedência sobre align-items/align-self
- suporte em row, row-reverse, column e column-reverse
- primeiro teste de conformidade Flexbox executável separado
- doutrina permanente WHATWG / CSSWG / WPT

NÃO FOI FEITO:
- representar auto com magic number
- cálculo Flexbox no Paint
- acesso DOM pelo Paint
- declarar Flexbox completo
- declarar WPT oficial passando sem executar WPT oficial
- margin:auto de Block Formatting

GATES:

cargo fmt --all
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p phantom-layout --test wpt_flexbox_slice

Somente se TODOS passarem:

taskkill /F /IM phantom-browser.exe
cargo build --release -p phantom-browser
.\target\release\phantom-browser.exe

PRÓXIMO MARCO RECOMENDADO:
2C-1 — Images + Replaced Elements Boundary.
