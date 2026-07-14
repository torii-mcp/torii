# Migração do AWS Gate

O Torii não migra configuração do AWS Gate automaticamente.

Executar `torii`, `torii init`, comandos de provider ou comandos de integração de agentes não lê nem copia `~/.config/.awsgate`. As variáveis `AWSGATE_CONFIG_DIR` e `AWSGATE_NO_GUI` também não são aliases do Torii.

Essa separação permite manter o AWS Gate em uso enquanto o Torii é desenvolvido e homologado. Os dois programas têm diretórios, regras, credenciais, grants, caches e logs independentes.

## Migração futura

Não existe comando de migração nesta versão. Quando esse fluxo for implementado, ele deve ser explícito, mostrar origem e destino, não alterar a origem e pedir confirmação antes de copiar regras ou credenciais.

Até lá, configure o Torii separadamente em `~/.config/torii` ou use `TORII_CONFIG_DIR` para um ambiente isolado de desenvolvimento.
