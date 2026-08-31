# Phantom 2D-6 — Parser/Layout & Supply-Chain Security Gate

## Escopo congelado

Esta etapa fecha somente SG-005 e SG-006 e introduz o contrato passivo `SecurityEvent` aprovado para o futuro Guardian.

## SG-005 — Parser/DOM/CSS/Layout

### HTML/DOM

- HTML source: 4 MiB
- DOM nodes: 65.536
- nesting depth: 256
- attributes/element: 128
- attribute bytes/element: 1 MiB
- raw start tag: 2 MiB
- text node: 1 MiB
- aggregate retained text: 3 MiB
- comment: 256 KiB
- style body: 1 MiB
- start-tag scanning respeita aspas e não encerra a tag em `>` dentro de valores citados
- raw-text close scanning sem `to_ascii_lowercase()` do restante inteiro

### Recalibração de compatibilidade HTML

Os budgets de start tag e atributos foram recalibrados após validação com HTML real de produção contendo grandes payloads de hidratação em `data-*`. O documento continua limitado a 4 MiB, o número de atributos continua limitado a 128 e entradas acima dos novos budgets continuam falhando de forma determinística.

### CSS

- source: 1 MiB
- scanned rule blocks: 2.048
- accepted rules: 1.024
- declarations/rule: 64
- selector bytes: 4 KiB
- selector parts: 32
- classes/compound selector: 32
- inline style: 64 KiB
- cascade rule evaluations/document: 2.000.000
- numeric magnitude: até 1.000.000
- NaN/Infinity rejeitados ou sanitizados antes do snapshot computado

### Layout

O layout continua usando snapshot frio e sanitização de viewport. A profundidade e o número de nós são limitados antes do layout; testes adversariais congelam esse contrato contra expansão recursiva não limitada.

## SG-006 — Supply chain

- Rust 1.95.0 pinado;
- `--locked` nos gates;
- cargo-audit 0.22.2;
- cargo-deny 0.20.2;
- Dependabot para Cargo e GitHub Actions;
- CODEOWNERS;
- Actions pinadas por SHA;
- `persist-credentials: false`;
- `permissions: {}` no topo;
- privilégio de escrita somente no job de publicação;
- cargo-auditable 0.7.5;
- provenance attestation;
- política de ruleset documentada para `main` e tags `v*`.

## Guardian

`SecurityEvent` é tipado, passivo e local. Não possui autoridade de policy/capability e não executa ações.

## Estado

**2D-6 implementation package: entregue.**

**Security Gate 2D: ainda NÃO é PASS** até os gates locais, CI, ruleset de `main` e ruleset `v*` serem confirmados.
