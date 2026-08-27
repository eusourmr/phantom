# Phantom Engine — Princípios de Arquitetura

Status: direção arquitetural do projeto.

## Filosofia central

O Phantom continuará desenvolvendo um motor web próprio, incremental e auditável.
Não usaremos Chromium, WebKit, Gecko, Servo ou WebView como renderer.
Bibliotecas externas podem existir somente em fronteiras estreitas de infraestrutura
quando isso não substitui o núcleo do motor (por exemplo TLS/HTTP e windowing).

## 1. Muralhas entre os estágios

Pipeline alvo:

Network -> HTML -> DOM -> CSSOM -> Style -> Layout -> Display List -> Paint -> Compositor

Regras:
- cada estágio tem contrato explícito;
- Layout nunca chama Paint;
- Paint nunca lê DOM;
- dados cruzam estágios como snapshots imutáveis;
- comunicação futura por filas/canais;
- nenhum estágio depende da implementação interna do estágio seguinte.

## 2. HTML vivo primeiro

Implementar um subconjunto pequeno, verificável e útil antes da cobertura ampla.
Prioridade inicial:
- div
- p
- span
- img
- a

Elementos já suportados podem permanecer, mas não ampliamos cobertura por quantidade.
Flexbox e posicionamento estático entram antes de Grid, floats, tabelas e positioning avançado.

## 3. Parser próprio

html5ever, Gumbo e cssparser podem ser usados como:
- referência de comportamento;
- comparação;
- geração de casos de teste;
- estudo de edge cases.

Eles não serão incorporados como parser principal do Phantom.
Tokenização, correção de erros e parsing evoluirão no próprio motor.

## 4. JavaScript isolado

JavaScript permanece OFF até existir uma arquitetura segura.
Quando chegar:
- não compartilhará a thread do compositor;
- não controlará Layout/Paint diretamente;
- mutações passam por mensagens/capabilities;
- scrolling/composição devem continuar responsivos se o script bloquear.

## 5. Snapshots imutáveis

Direção:
- árvores/snapshots versionados;
- Arc e copy-on-write onde fizer sentido;
- Style/Layout trabalham sobre uma versão estável;
- cálculos antigos podem ser descartados sem corromper o estado atual.

## 6. Standards Mode primeiro

Primeira meta de compatibilidade:
- HTML Standards Mode;
- DOCTYPE moderno;
- sem Quirks Mode inicialmente;
- sem carregar compatibilidade histórica como requisito prematuro.

## 7. GPU como destino do renderer

O egui atual é shell/prova de conceito, não o renderer web definitivo.

Destino:
DOM/CSSOM -> Layout Tree -> Display List -> Paint Commands -> GPU Compositor

WGPU/WebGPU é a direção preferencial de abstração GPU.
Rasterização de software poderá existir para testes/headless/fallback, não como arquitetura principal.

## 8. Test harness antes da expansão

Criar harness headless cedo.
WPT será usado progressivamente para:
- parsing;
- DOM;
- CSS;
- layout;
- comportamento web.

Cada feature nova deve entrar com testes antes ou junto da implementação.
Testes visuais/reftests serão adicionados quando Paint/Layout tiver geometria determinística.

## 9. JavaScript engine: decisão futura, núcleo protegido

QuickJS/JavaScriptCore/V8 podem ser estudados como referências ou possíveis runtimes isolados.
Nenhum deles entra agora no núcleo.
A decisão só será tomada depois de DOM, Style, Layout, Paint e compositor terem contratos estáveis.

## 10. Layout Tree é produto próprio e frio

A DOM não será usada diretamente pelo renderer.

Objetivo:
DOM + Computed Style
        ->
Layout Snapshot frio
        ->
arrays/vetores contíguos
        ->
geometria + referências de recursos
        ->
Display List

Princípios:
- sem ponteiros diretos para DOM;
- Data-Oriented Design;
- IDs/índices estáveis;
- dados compactos;
- cache locality;
- snapshot imutável.

## Prioridade técnica

Security
-> Correctness
-> Privacy
-> Human control
-> Standards compatibility
-> Performance
-> Memory efficiency

Nenhuma otimização justifica enfraquecer invariantes de segurança ou correção.
