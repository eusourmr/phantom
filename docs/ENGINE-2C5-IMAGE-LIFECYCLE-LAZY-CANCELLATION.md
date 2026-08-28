# Phantom Engine 2C-5 — Image Lifecycle + Lazy Loading + Cancellation

## Objetivo

Controlar quando imagens são buscadas, quando o trabalho deixa de ter validade
e quando animações devem consumir tempo de execução, sem mover lifecycle para
DOM, LayoutSnapshot ou Paint.

## Implementação

- `loading="lazy"` é normalizado pelo engine como hint tipado.
- Imagens eager têm prioridade sobre imagens lazy.
- Imagens lazy próximas ao viewport inicial são carregadas antecipadamente.
- Imagens lazy distantes permanecem adiadas até se aproximarem do viewport.
- Cada navegação cria uma nova geração documental monotônica.
- Eventos de imagem carregam a geração que os originou.
- Resultados de gerações anteriores nunca são instalados no documento atual.
- Navegar, recarregar ou fechar a aba cancela o batch de imagens vigente.
- Workers observam um token atômico antes e depois de cada fetch/decode.
- Animações fora do viewport e em abas inativas congelam seu relógio e deixam
  de solicitar repaint.
- O cache raster continua bounded a 256 MiB por aba.

## Limites arquiteturais

- DOM preserva somente atributos HTML.
- LayoutSnapshot preserva somente geometria e identificadores opacos.
- Paint continua renderer-neutral e recebe somente o recurso visual atual.
- O browser/resource runtime possui geração, cancelamento, fila lazy,
  prioridade, visibilidade e relógios de animação.
- Cancelamento é cooperativo; uma chamada de rede síncrona já em andamento pode
  terminar internamente, mas seu resultado não atravessa a geração documental.

## Gates

```bash
cargo fmt --all
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p phantom-image --test animation_decode
cargo test -p phantom-image --test raster_decode
cargo build --release -p phantom-browser
```

## Aceite nativo

1. Navegar durante carregamento não instala imagens da página anterior.
2. Recarregar invalida o batch anterior.
3. `loading="lazy"` distante não inicia imediatamente.
4. Rolagem aproxima e ativa imagens adiadas.
5. Animação fora da tela congela e retoma sem salto temporal.
6. Aba inativa não mantém timeline ou repaint de animação.
7. Fechar aba encerra cooperativamente o worker.

## Estado da candidata

Em 28/08/2026, formatação, Clippy com warnings negados, testes do workspace,
testes dedicados de animação/raster e build release passaram com Rust 1.95.
A homologação final depende do aceite nativo no Windows descrito acima.
