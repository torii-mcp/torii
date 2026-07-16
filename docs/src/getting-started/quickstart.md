# Primeiros passos

Este fluxo usa configuração isolada e um pacote local do repositório para permitir revisão antes da execução.

## 1. Inicializar a raiz

```powershell
$env:TORII_CONFIG_DIR = "$PWD/.torii-dev"
cargo run -- init
```

`init` cria apenas `settings.yaml` e a estrutura base. Providers são instalados explicitamente.

## 2. Instalar providers

Durante o desenvolvimento local:

```powershell
cargo run -- provider install ./examples/providers/aws
cargo run -- provider install ./examples/providers/kubectl
cargo run -- provider list
```

Em uma distribuição configurada com o catálogo canônico, use apenas `provider install aws` ou pesquise com `provider search`.

Após a instalação, ambos os `rules.yaml` estão vazios. Nenhuma operação do agente atravessa por padrão.

## 3. Aplicar um setup opcional

```powershell
cargo run -- provider setup aws readonly
cargo run -- provider setup kubectl readonly
```

O setup aplica a política curada somente se rules ainda estiver vazio. Revise a política AWS conforme sua classificação de dados.

## 4. Criar target Kubernetes

```powershell
cargo run -- target add kubectl meu_dev --context meu-context-real --provider aws
```

## 5. Preparar uma sessão AWS

```powershell
cargo run -- reauth aws
```

Torii valida a candidata com `aws sts get-caller-identity` antes de substituir a sessão anterior.

## 6. Iniciar o MCP

```powershell
cargo run
```

Normalmente o cliente MCP inicia esse processo. Em headless, `TORII_NO_GUI=1` nega chamadas não resolvidas e cancela coleta de autenticação com segurança.
