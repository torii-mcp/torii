# Layout de configuração

O diretório padrão é `~/.config/torii`. `TORII_CONFIG_DIR` substitui a raiz inteira.

```text
~/.config/torii/
├── settings.yaml
├── torii.log
└── providers/
    ├── aws/
    │   ├── provider.yaml
    │   ├── rules.yaml
    │   ├── .env
    │   ├── .torii-package/
    │   │   ├── manifest.yaml
    │   │   ├── lock.yaml
    │   │   └── content/
    │   ├── grants
    │   └── identities/
    │       └── aws/
    │           ├── .session-cache
    │           ├── .identity-cache
    │           └── auth/credentials.env
    ├── kubectl/
    │   ├── provider.yaml
    │   ├── rules.yaml
    │   ├── .env
    │   ├── .target-authorizations.yaml
    │   ├── .target-authorizations.lock
    │   └── targets/
    │       └── lab/
    │           ├── target.yaml
    │           ├── rules.yaml
    │           ├── .env
    │           └── grants
    └── aws-profile/
        ├── provider.yaml
        ├── .target-authorizations.yaml
        ├── .target-authorizations.lock
        ├── identities/
        │   └── empresa-producao/
        │       ├── .session-cache
        │       ├── .identity-cache
        │       └── auth/
        └── targets/
            └── producao/
                └── target.yaml
```

`.torii-package/` existe somente em instalações gerenciadas e contém origem, digest e setups. `rules.yaml` e `.env` ficam fora desse diretório porque pertencem ao operador. Rules do target é opcional e substitui o compartilhado; `.env` do target sobrepõe chaves compartilhadas. Sessão, cache, `.identity-cache`, credenciais e lock vivem sempre no provider de identidade em `identities/<scope>/`, isolados por escopo — não no diretório do target. Um provider não target-aware autentica num escopo com o nome da própria tool; targets isolam por `identity.scope` (default: nome do target) e só compartilham o balde quando declaram o mesmo escopo.

`.target-authorizations.yaml` é diferente de `grants`: é o estado de lease dos aliases daquele provider. Ele guarda versão, revisão, alias, digest do binding e expiração. O digest faz um lease expirar logicamente quando o `target.yaml` correspondente muda, mesmo antes do horário previsto. `.target-authorizations.lock` é o arquivo usado para lock entre processos durante mudanças nesse estado: usa exclusão do sistema operacional (share lock no Windows e `flock` no Unix), e o handle é liberado automaticamente se o processo termina ou falha. Não há TTL ou limpeza por timeout de um lock considerado stale; não o crie, remova ou sincronize manualmente.

## `settings.yaml`

```yaml
max_output_bytes: 262144
default_grant_minutes: 2
default_target_minutes: 15
```

`max_output_bytes` limita o conteúdo combinado devolvido; `default_grant_minutes` define o valor inicial da aprovação temporária de uma operação; `default_target_minutes` define o valor inicial do lease de um alias target-aware. Leases aceitam de 1 a 1.440 minutos.

## Arquivos gerenciados

- `grants`: arquivo YAML versão `2` com expiração e fingerprint do matcher; o formato legado de epoch, tab e regra não é reutilizado;
- `.target-authorizations.yaml`: leases temporários por provider, com revisão e digest do binding; não concede operações Jasper;
- `identities/<scope>/.session-cache`: epoch da última validação da sessão daquele escopo;
- `identities/<scope>/.identity-cache`: epoch e identidade confirmada pelo probe `auth.identity`;
- `identities/<scope>/auth/credentials.env`: material sensível;
- `torii.log`: auditoria append-only best-effort.

Restrinja acesso ao diretório com permissões do sistema operacional e não o sincronize em repositórios ou telemetria.
