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
    │   ├── .session-cache
    │   └── auth/credentials.env
    └── kubectl/
        ├── provider.yaml
        ├── rules.yaml
        ├── .env
        └── targets/
            └── lab/
                ├── target.yaml
                ├── rules.yaml
                ├── .env
                └── grants
```

`.torii-package/` existe somente em instalações gerenciadas e contém origem, digest e setups. `rules.yaml` e `.env` ficam fora desse diretório porque pertencem ao operador. Rules do target é opcional e substitui o compartilhado; `.env` do target sobrepõe chaves compartilhadas. O target guarda somente política, grants e ambiente próprios. Cache, lock e credenciais pertencem ao provider indicado pelo target.

## `settings.yaml`

```yaml
max_output_bytes: 262144
default_grant_minutes: 2
```

`max_output_bytes` limita o conteúdo combinado devolvido; `default_grant_minutes` define o valor inicial da aprovação temporária.

## Arquivos gerenciados

- `grants`: arquivo YAML versão `2` com expiração e fingerprint do matcher; o formato legado de epoch, tab e regra não é reutilizado;
- `.session-cache`: epoch da última validação;
- `auth/credentials.env`: material sensível;
- `torii.log`: auditoria append-only best-effort.

Restrinja acesso ao diretório com permissões do sistema operacional e não o sincronize em repositórios ou telemetria.
