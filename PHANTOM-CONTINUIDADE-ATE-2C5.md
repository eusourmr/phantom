# PHANTOM — CONTINUIDADE TÉCNICA ATÉ 2C-5

## Estado em 28/08/2026

Baseline oficial: **2C-4 — Animated Image Timeline + GIF / Animated WebP**, homologada no Windows por Ricardo Rocha.

Etapa atual: **2C-5 — Image Lifecycle + Lazy Loading + Cancellation**.

Status: **implementada, gates automatizados aprovados e build release aprovado; candidata à homologação nativa no Windows**.

## Entregas 2C-5

- `ImageLoading::{Eager, Lazy}` como tipo de domínio no `phantom-engine`.
- normalização de `loading="lazy"`; valores ausentes ou inválidos são eager;
- geometria da imagem exposta no request sem acoplar Layout ao browser;
- eager antes de lazy;
- lazy próximo ao viewport recebe preload bounded;
- lazy distante permanece em fila até aproximação por rolagem;
- geração documental monotônica por navegação/reload;
- todo evento de imagem carrega sua geração de origem;
- eventos obsoletos não são instalados;
- nova navegação substitui a anterior e invalida seus resultados;
- batch de imagens possui token atômico de cancelamento;
- fechar aba, navegar ou recarregar cancela trabalho de imagem;
- cache continua limitado a 256 MiB por aba;
- animações fora do viewport e em abas inativas congelam o relógio;
- somente animações visíveis da aba ativa solicitam repaint.

## Arquitetura preservada

- DOM guarda somente atributos HTML;
- LayoutSnapshot guarda geometria e IDs opacos;
- Paint não controla fetch, lifecycle, frame ou relógio;
- browser/resource runtime controla prioridade, geração, cancelamento, lazy queue,
  visibilidade e timeline;
- cancelamento de rede é cooperativo: fetch síncrono iniciado pode terminar
  internamente, mas nunca instala resultado em geração inválida.

## Gates aprovados com Rust 1.95

```text
cargo fmt --all                                       PASS
cargo fmt --all --check                               PASS
cargo clippy --workspace --all-targets -- -D warnings PASS
cargo test --workspace                                PASS
cargo test -p phantom-image --test animation_decode   PASS (3/3)
cargo test -p phantom-image --test raster_decode      PASS (3/3)
cargo build --release -p phantom-browser              PASS
```

Testes novos:

- normalização eager/lazy no engine;
- eager priorizado antes de lazy;
- descarte do batch aciona cancelamento;
- relógio da animação congela quando inativo.

## Homologação nativa exigida

```powershell
cargo fmt --all
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
taskkill /F /IM phantom-browser.exe
cargo build --release -p phantom-browser
.\target\release\phantom-browser.exe
```

Validar:

1. navegar durante carregamento não mistura imagens de documentos;
2. reload invalida o batch anterior;
3. imagem lazy distante não baixa imediatamente;
4. rolagem ativa a imagem lazy ao aproximá-la;
5. GIF/WebP fora da tela congela e retoma sem salto;
6. animação em aba inativa congela;
7. fechar aba não mantém instalação de recursos.

Somente após esse aceite: **2C-5 HOMOLOGADA**.

Não iniciar 2C-6 antes da homologação.
