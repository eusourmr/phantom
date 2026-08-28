# Phantom — Continuidade após 2C-10

## Baseline anterior

- 2C-7: Image Recovery + HTTP Cache Revalidation — homologada.
- 2C-8: Network Isolation Key + Cache Partitioning.
- 2C-9: Resource Priority + Preload Scheduling + Chrome UX I; tema do sistema, tabs fixadas, menus/atalhos e refinamentos de window controls.

## 2C-10

### Motor

**Navigation Robustness I — Redirect Guardrails**

O transporte não segue mais redirects documentais de forma opaca. O Phantom valida cada hop, limita a cadeia, detecta loop e mantém a fronteira HTTP/HTTPS em todos os destinos.

### Navegador/UX

**Recently Closed Tabs**

Pilha bounded de 10 tabs, `Ctrl+Shift+T` e controle visual de recuperação no chrome.

## Regra oficial de roadmap a partir daqui

Cada versão 2C-n terá duas colunas:

1. **Motor / Web Platform / segurança / performance** — uma evolução técnica coesa.
2. **Browser / Chrome / UX** — uma evolução pequena, observável e relacionada quando possível.

Nenhuma das duas colunas deve justificar aumento desnecessário de escopo da outra.

## Próximo marco proposto

### 2C-11 — Document Cache Semantics + Site Identity I

Motor:
- cache HTTP de documentos em memória;
- `Cache-Control`, ETag e Last-Modified para navegação;
- reload força revalidação;
- navegação normal pode reutilizar representação fresh;
- integração coerente com redirect final URL.

Browser/UX:
- primeira identidade real do site na aba: descoberta bounded de favicon (`<link rel=icon>`), fallback seguro e uso especialmente útil em tabs fixadas.

A 2C-11 só deve iniciar após homologação da 2C-10.
