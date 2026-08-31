# Phantom Guardian — SecurityEvent Contract (2D-6)

## Status

Contrato tipado e passivo. **Sem IA e sem automação nesta etapa.**

## Regra arquitetural

A segurança obrigatória do Phantom continua determinística. `SecurityEvent` existe apenas para permitir que subsistemas emitam observações estruturadas para uma futura camada Guardian.

Criar ou transportar um `SecurityEvent`:

- não concede `Capability`;
- não executa comando;
- não acessa filesystem;
- não inicia rede;
- não muda policy;
- não aprova permissão;
- não transmite conteúdo a terceiros.

## Tipos

`SecuritySurface` identifica a origem lógica: Parser, Style, Layout, Network, Permission ou SupplyChain.

`SecurityEventCode` é um conjunto fechado de categorias estáveis, incluindo budget excedido, mixed content, private network, permission denied e supply-chain gate failure.

`SecuritySeverity` permite priorização local: Info, Warning e High.

`SecurityEvent` contém apenas os tipos acima e contexto local opcional limitado a 512 bytes.

## Evolução

Na linha 2I, o contrato poderá ser conectado às interfaces de security/origin/permissions/isolation. A camada Guardian continua sem autoridade para substituir o kernel determinístico.
