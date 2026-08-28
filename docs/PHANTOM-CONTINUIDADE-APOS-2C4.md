# PHANTOM — CONTINUIDADE APÓS A CORREÇÃO 2C-4

## Estado em 28/08/2026

Baseline oficial anterior: **2C-3 homologada sem erros**.

Estado da 2C-4: **implementação corrigida e gates automatizados homologados**.
Falta somente o smoke test gráfico do executável nativo no Windows para mudar
o baseline oficial integral para 2C-4.

## Correção concluída

- `image = "=0.25.10"` está alinhada em `phantom-image` e `phantom-browser`.
- Foram removidos `image::metadata::LoopCount` e `decoder.loop_count()`.
- O loop GIF é lido de `NETSCAPE2.0` ou `ANIMEXTS1.0` por parser mínimo bounded.
- O loop Animated WebP é lido do chunk `ANIM` por parser mínimo bounded.
- GIF sem extensão de loop executa uma vez.
- GIF finito converte reinícios em execuções totais; WebP finito já informa as
  execuções totais.
- DOM, LayoutSnapshot e Paint continuam sem frames, relógio ou estado de animação.

## Gates homologados com Rust 1.95

```text
cargo fmt --all                                       PASS
cargo fmt --all --check                               PASS
cargo clippy --workspace --all-targets -- -D warnings PASS
cargo test --workspace                                PASS
cargo test -p phantom-image --test animation_decode   PASS (3/3)
cargo test -p phantom-image --test raster_decode      PASS (3/3)
cargo build --release -p phantom-browser              PASS
```

## Aceite final no Windows

```powershell
taskkill /F /IM phantom-browser.exe
cargo build --release -p phantom-browser
.\target\release\phantom-browser.exe
```

Validar GIF e Animated WebP infinitos e finitos. Se todos animarem e os finitos
pararem no último frame: **2C-4 HOMOLOGADA; novo baseline oficial 2C-4**.

Não iniciar 2C-5 antes desse smoke test.
