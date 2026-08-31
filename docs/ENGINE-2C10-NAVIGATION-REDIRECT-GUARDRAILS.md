# Phantom 2C-10 — Navigation Robustness I: Redirect Guardrails

## Objetivo

Mover redirects de documento para uma política explícita do Phantom em vez de delegar a cadeia ao transporte HTTP.

## Entregas

- `ureq` deixa de seguir redirects automaticamente (`max_redirects(0)`).
- Phantom segue explicitamente apenas `301`, `302`, `303`, `307` e `308`.
- Cada `Location` é resolvido por `HttpUrl`, preservando a fronteira HTTP/HTTPS e rejeitando credenciais embutidas.
- redirects relativos são suportados.
- cadeia limitada a 10 hops.
- detecção de loops antes de repetir I/O para uma URL já visitada.
- redirect sem `Location` utilizável gera erro tipado.
- `304` standalone no pipeline documental é rejeitado.
- `TextResponse::redirect_count()` expõe o número de hops seguidos.

## Não entra nesta versão

- cache HTTP de documentos;
- revalidação de documentos com ETag/Last-Modified;
- cache persistente;
- Service Workers;
- cookies;
- HSTS.

Esses itens permanecem separados para evitar misturar política de redirects com semântica de cache documental.

## Testes determinísticos

`crates/phantom-net/tests/navigation_redirects.rs` cobre:

1. redirect relativo seguido até 200 final;
2. contagem de redirects;
3. loop `A -> B -> A` rejeitado antes da terceira requisição;
4. redirect apenas de fragmento tratado como o mesmo alvo de rede;
5. cadeia limitada a 10 hops;
6. destino fora de HTTP/HTTPS rejeitado;
7. redirect sem `Location` rejeitado.
