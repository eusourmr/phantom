# Diretrizes do Projeto — Navegador Web em Rust

> Este documento define os padrões técnicos, arquiteturais e de processo deste projeto.
> Objetivo: código sustentável, leve, bem documentado, organizado e no nível de um
> engenheiro sênior/staff. Todo colaborador (incluindo você mesmo, daqui a 1 ano) deve
> seguir isso.

---

## 1. Filosofia do projeto

- **Simples vence esperto.** Prefira código óbvio a código "inteligente". Se precisar de
  um comentário para explicar *o que* o código faz, reescreva o código — não o comentário.
- **Correção > velocidade de entrega.** Um navegador lida com input adversarial (HTML/CSS/JS
  da internet inteira). Trate todo parser como hostil por padrão.
- **Sem dependências desnecessárias.** Cada crate externa é superfície de ataque, peso de
  build e dívida de manutenção. Justifique cada uma.
- **Zero warnings, zero `unsafe` não documentado.** Qualidade não é opcional, é o padrão.
- **Módulos pequenos e coesos.** Se um arquivo passa de ~400-500 linhas, provavelmente
  está fazendo coisa demais.

---

## 2. Estrutura do repositório (Cargo workspace)

Um navegador não é um binário só — é um conjunto de subsistemas independentes. Use um
**workspace** com uma crate por responsabilidade. Isso força separação de conceitos,
acelera builds incrementais e permite testar cada camada isoladamente.

```
meu-navegador/
├── Cargo.toml                 # workspace root
├── README.md
├── CHANGELOG.md
├── LICENSE
├── CONTRIBUTING.md
├── rust-toolchain.toml        # fixa a versão do Rust (reprodutibilidade)
├── .cargo/config.toml
├── .github/
│   └── workflows/
│       ├── ci.yml              # build + test + clippy + fmt
│       └── audit.yml           # cargo-audit / cargo-deny agendado
├── docs/
│   ├── architecture.md         # visão geral do sistema (com diagramas)
│   ├── adr/                    # Architecture Decision Records
│   │   ├── 0001-workspace-layout.md
│   │   └── 0002-parser-strategy.md
│   └── rfcs/                   # propostas de mudanças grandes, discutidas antes de implementar
├── crates/
│   ├── net/                    # rede: TCP/TLS, HTTP/1.1, HTTP/2, cache, cookies
│   ├── html/                   # tokenizer + parser HTML5 (spec whatwg)
│   ├── css/                    # tokenizer + parser CSS, seletores, especificidade
│   ├── dom/                    # estrutura de árvore DOM, tipos de nó
│   ├── style/                  # cascade, resolução de estilos computados
│   ├── layout/                 # box model, flow layout, flexbox etc.
│   ├── render/                 # rasterização / pintura (ex: via wgpu ou tiny-skia)
│   ├── js/                     # engine JS ou binding para uma existente (ex: boa, deno_core)
│   ├── platform/               # abstração de janelas/eventos (ex: winit)
│   └── browser-core/           # orquestra tudo: event loop, tabs, navegação
├── xtask/                      # automação de build/tarefas (padrão da comunidade Rust)
└── fuzz/                       # alvos de fuzzing (cargo-fuzz) para os parsers
```

**Regras:**
- Cada crate tem seu próprio `Cargo.toml`, seu próprio `README.md` curto e seus próprios testes.
- Dependência só flui em uma direção: `net → html/css → dom → style → layout → render`.
  Nunca crie dependência circular entre crates.
- `browser-core` é a única crate que conhece todas as outras. As demais não devem
  conhecer `browser-core`.

---

## 3. Padrões de código

- **`rustfmt` obrigatório**, com `rustfmt.toml` versionado no repo (não confie no default
  implícito — declare explicitamente):
  ```toml
  edition = "2021"
  max_width = 100
  imports_granularity = "Module"
  group_imports = "StdExternalCrate"
  ```
- **`clippy` no nível `-D warnings`** rodando no CI. Nada de merge com warnings.
- **Nomenclatura**: siga as convenções oficiais do Rust API Guidelines
  (`snake_case` para funções/variáveis, `UpperCamelCase` para tipos, sem abreviações
  obscuras — `parse_html`, não `p_htm`).
- **Sem `unwrap()`/`expect()` em código de produção**, exceto em `main()`, testes, ou
  quando a invariante for logicamente impossível de falhar — e nesse caso, `expect("motivo claro")`,
  nunca `unwrap()` cru.
- **Erros tipados**, não `String` genérica. Use `thiserror` para erros de biblioteca e
  `anyhow` só no nível de aplicação (o binário final), nunca dentro das crates de motor.
- **`unsafe` é exceção, não regra.** Todo bloco `unsafe` precisa de comentário
  `// SAFETY: ...` explicando por que é seguro. Rode `cargo geiger` periodicamente
  para visualizar uso de unsafe em dependências também.
- **Sem `mod.rs`** — use o estilo moderno (`meu_modulo.rs` + pasta `meu_modulo/`), mais
  fácil de navegar em editores.

---

## 4. Documentação

Documentação é tratada como código: revisada em PR, versionada, testada.

- **Rustdoc em tudo que é público.** Todo `pub fn`, `pub struct`, `pub enum` tem doc
  comment (`///`) explicando propósito, não repetindo a assinatura. Exemplos de uso
  dentro do doc comment (viram doctests automaticamente):
  ```rust
  /// Parseia uma folha de estilo CSS bruta em uma lista de regras.
  ///
  /// Segue o algoritmo de tokenização definido na CSS Syntax Module Level 3.
  /// Erros de sintaxe individuais são recuperados de acordo com a spec
  /// (regras inválidas são descartadas, não abortam o parse inteiro).
  ///
  /// # Exemplo
  /// ```
  /// let sheet = css::parse("body { color: red; }");
  /// assert_eq!(sheet.rules.len(), 1);
  /// ```
  pub fn parse(input: &str) -> Stylesheet { ... }
  ```
- **`#![warn(missing_docs)]`** no topo de cada crate de biblioteca — força documentar
  toda API pública ou o build falha.
- **`cargo doc --no-deps --open`** deve gerar uma documentação navegável e completa.
  Publique via GitHub Pages a cada release.
- **README por crate**: o que a crate faz, por que existe separada, com quem ela se
  relaciona no pipeline.
- **`docs/architecture.md`**: visão de alto nível de como uma requisição HTTP vira
  pixels na tela (diagrama do pipeline: rede → parse → DOM → CSSOM → style → layout
  → paint). Esse é o documento que qualquer novo colaborador lê primeiro.
- **ADRs** (`docs/adr/`): toda decisão estrutural relevante ("por que escolhemos um
  parser recursivo-descendente em vez de gerado", "por que não usamos servo diretamente")
  vira um arquivo curto e numerado. Nunca apague um ADR antigo, mesmo que a decisão
  tenha mudado — marque como "Superseded by ADR-00XX".
- **CHANGELOG.md** seguindo [Keep a Changelog](https://keepachangelog.com), atualizado
  a cada PR relevante, não só na hora do release.

---

## 5. Testes

Um navegador vive ou morre pela cobertura de testes, porque a superfície de entrada
(HTML/CSS/JS arbitrário da web) é imprevisível.

- **Testes unitários** ao lado do código (`#[cfg(test)] mod tests`) para lógica interna.
- **Testes de integração** (`tests/`) por crate, testando a API pública.
- **Testes de conformidade com specs**: sempre que possível, rode os *test suites*
  oficiais (ex: [Web Platform Tests](https://github.com/web-platform-tests/wpt) para
  HTML/CSS/DOM). Isso é o que separa um brinquedo de um motor de navegador de verdade.
- **Fuzzing nos parsers** (`cargo-fuzz` + `libFuzzer`) — HTML, CSS e headers HTTP são os
  alvos mais críticos, pois processam dados não confiáveis.
- **Testes de regressão visual** (snapshot testing) para o pipeline de layout/render,
  comparando screenshots renderizados contra um baseline aprovado.
- **CI roda tudo em cada PR**: `cargo test --workspace`, `cargo clippy --workspace -- -D warnings`,
  `cargo fmt --check`, e periodicamente `cargo audit` / `cargo deny check`.

---

## 6. Dependências e segurança de supply chain

- **Minimalismo deliberado**: antes de adicionar uma crate, pergunte "eu consigo
  escrever isso em <200 linhas sem dependência?". Um navegador do zero é, em parte,
  um exercício de não depender de tudo pronto.
- `cargo-deny` configurado (`deny.toml`) para bloquear licenças incompatíveis,
  duplicação excessiva de versões e crates com advisories de segurança conhecidos.
- `cargo-audit` rodando semanalmente via GitHub Actions agendado.
- `Cargo.lock` **sempre commitado**, mesmo sendo um projeto com crates de biblioteca
  (garante builds reprodutíveis do binário final).
- Prefira crates com poucas dependências transitivas e manutenção ativa a alternativas
  "populares porém infladas".

---

## 7. Performance

- Meça antes de otimizar: `criterion` para benchmarks de funções críticas (parser,
  layout), `cargo flamegraph` para achar hotspots reais.
- Evite alocação desnecessária nos hot paths (parser e layout rodam por *byte* de
  entrada). Prefira `&str`/slices a `String` sempre que possível; considere arenas
  (`bumpalo`) para árvores de curta duração como a AST do parser.
- Documente decisões de performance como ADR quando envolverem trade-off de legibilidade
  (ex: loop manual em vez de iterador por causa de benchmark comprovado — cite o número).

---

## 8. Versionamento e commits

- **Conventional Commits** (`feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`)
  — permite gerar changelog automaticamente e comunica intenção no histórico.
- **SemVer real** para cada crate do workspace, não só para o binário final.
- PRs pequenos e focados. Um PR = uma mudança logicamente coesa, com descrição do
  "porquê", não só do "o quê" (isso o diff já mostra).
- Squash-merge com mensagem final revisada, mantendo histórico limpo na branch principal.

---

## 9. Licença e governança

- Escolha e declare a licença logo no início (`LICENSE` na raiz) — MIT/Apache-2.0
  dual-license é o padrão de facto do ecossistema Rust.
- `CONTRIBUTING.md` explicando como rodar o projeto localmente, rodar testes, e o
  processo de review.
- `CODE_OF_CONDUCT.md` se o projeto for aceitar contribuições externas.

---

## 10. Checklist rápido antes de todo merge

- [ ] `cargo fmt --check` passa
- [ ] `cargo clippy --workspace -- -D warnings` limpo
- [ ] `cargo test --workspace` passa
- [ ] Toda API pública nova tem doc comment com exemplo
- [ ] Nenhum `unwrap()`/`expect()` sem justificativa em código de produção
- [ ] Todo `unsafe` novo tem comentário `// SAFETY:`
- [ ] ADR criado se a mudança for arquitetural
- [ ] CHANGELOG.md atualizado
- [ ] Sem dependência nova sem justificativa no PR

---

*Este documento é vivo — atualize-o conforme o projeto evolui. Decisões que o
contradigam devem primeiro alterá-lo (via PR), não apenas serem feitas no código.*
