# Phantom 2C-10 — Chrome UX II: Recently Closed Tabs

## Regra de evolução

A partir da 2C-10, cada avanço estrutural do motor é acompanhado por um avanço pequeno e delimitado do navegador/UX. O objetivo é evoluir infraestrutura e experiência em paralelo sem transformar cada versão em uma reforma ampla.

## Entrega desta versão

- pilha bounded das 10 últimas abas fechadas que possuíam URL;
- preservação de URL atual, título, histórico local, posição do histórico e estado `pinned`;
- `Ctrl+Shift+T` reabre a última aba fechada;
- quando existe aba recuperável, aparece no chrome um controle Lucide `History` ao lado da criação de aba;
- o controle exibe `Reabrir última aba fechada · Ctrl+Shift+T`;
- aba fixada recuperada volta à região das tabs fixadas;
- a reabertura navega novamente para o documento e não tenta ressuscitar snapshots de DOM/layout/texturas obsoletos.

## Decisão arquitetural

O histórico de abas fechadas pertence ao browser shell. `phantom-engine` não conhece abas, atalhos ou sessão de navegador.

A recuperação deliberadamente guarda estado de navegação, não o objeto `Engine`. Isso evita manter DOM, raster e texturas de páginas fechadas apenas para permitir reopen.

## Iconografia

Nenhum novo pacote foi adicionado. O controle usa a biblioteca Lucide já atribuída em `docs/THIRD-PARTY-ATTRIBUTIONS.md`.
