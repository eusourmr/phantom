# GitHub Main/Tag Protection — 2D-6

## Objetivo

Fechar o bloqueador SG-006 no plano de governança do repositório.

## Ruleset obrigatório para `main`

Configurar no GitHub um ruleset ativo que:

- bloqueie branch deletion;
- bloqueie force push;
- exija Pull Request para merge;
- exija status checks da CI;
- exija branch atualizada antes do merge;
- exija conversas resolvidas;
- restrinja bypass ao mínimo necessário.

O workflow CI desta 2D-6 usa permissões vazias por padrão, `contents: read` apenas no job, checkout por SHA imutável e `persist-credentials: false`.

## Ruleset obrigatório para tags `v*`

- bloquear deleção/recriação arbitrária de tags de release;
- restringir criação/atualização de `v*` ao fluxo autorizado;
- manter publicação de release em job separado.

## Antes do Beta

Ativar também **Private Vulnerability Reporting** no repositório.

## Gate

A presença destes arquivos no repositório não ativa rulesets automaticamente. O Security Gate 2D só pode ser marcado PASS depois da confirmação visual/administrativa de que os rulesets estão ativos no GitHub.
