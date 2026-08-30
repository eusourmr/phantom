# Phantom Security Fuzz Foundation — 2D-6

A 2D-6 não introduz um fuzzer como dependência do workspace. Ela congela os limites determinísticos que a futura infraestrutura de fuzz deverá preservar.

## Alvos iniciais

1. `phantom_html::parse`
2. `phantom_css::Stylesheet::parse`
3. pipeline `HTML -> DOM -> style -> layout` via `phantom_engine::Engine::load_html`

## Invariantes

- nenhum panic;
- nenhum `unsafe` no código Phantom;
- inputs acima dos budgets falham ou são ignorados de forma determinística;
- profundidade HTML nunca excede 256 elementos abertos;
- DOM nunca retém mais de 65.536 nós;
- CSS nunca aceita mais de 1.024 regras nem avalia indefinidamente;
- números não finitos não chegam ao snapshot computado/layout;
- corpus malformado não autoriza rede, filesystem, capability ou execução.

## Seeds

`security/fuzz-seeds/html/` contém casos pequenos e versionáveis para regressão: profundidade, fanout de atributos, raw text case-insensitive, comentários e CSS adversarial.

## Próxima evolução permitida

A infraestrutura de fuzz contínuo pode ser adicionada em uma etapa posterior seguindo o mesmo contrato, sem substituir os testes adversariais determinísticos desta 2D-6.
