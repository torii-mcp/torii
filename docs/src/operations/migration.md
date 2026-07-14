# Migração do AWS Gate

Na inicialização com o diretório padrão, Torii procura `~/.config/.awsgate` somente quando `providers/aws/provider.yaml` ainda não existe.

## Conteúdo migrado

| AWS Gate | Torii |
|---|---|
| `rules.yaml` | `providers/aws/rules.yaml` |
| `.env` | `providers/aws/.env` |
| `auth.env` | `providers/aws/auth/credentials.env` |
| `aws.env` legado | fallback para `auth/credentials.env` |
| `grants` | `providers/aws/grants` |

Torii cria `provider.yaml` a partir do exemplo AWS atual.

## Conteúdo não ativado

O cache antigo de sessão não é migrado. A credencial copiada precisa passar por `aws sts get-caller-identity` antes de ser considerada válida. O log antigo permanece no diretório legado e não é incorporado ao log novo.

## Garantias

- o diretório AWS Gate é somente lido;
- a montagem acontece em diretório temporário;
- o destino final é renomeado após o staging;
- configuração Torii existente nunca é sobrescrita;
- definir `TORII_CONFIG_DIR` ou `AWSGATE_CONFIG_DIR` desativa a migração automática do diretório padrão.

Após a primeira inicialização, revise `providers/aws/provider.yaml`, confirme as regras e execute `torii reauth aws` se a validação da sessão migrada falhar.

