# Pacotes e catálogo de providers

Um pacote de provider é declarativo: YAML, ambiente inicial e setups de política. Ele não pode conter nem executar scripts de instalação ou binários.

## Estrutura

```text
aws/
├── manifest.yaml
├── provider.yaml
├── rules.yaml
├── env.example
└── setups/
    └── readonly/
        └── rules.yaml
```

O `rules.yaml` da raiz é obrigatório e deve ter `accept` e `deny` vazios. O instalador recusa o pacote inteiro se essa condição não for satisfeita.

```yaml
version: "1"
name: aws
package_version: "0.1.0"
description: AWS CLI com autenticação temporária.
provider: provider.yaml
rules: rules.yaml
environment: env.example
setups:
  - name: readonly
    kind: readonly
    description: Descoberta e leitura operacional comum.
    rules: setups/readonly/rules.yaml
```

Podem existir vários setups, mas o único `kind` implementado é `readonly`. O Torii valida estrutura e regras mínimas; a afirmação de que uma operação é realmente read-only depende da revisão do pacote e do CLI real.

## Fontes de instalação

```powershell
torii provider install ./provider
torii provider install ./provider.zip
torii provider install ./provider.tar
torii provider install ./provider.tar.gz
torii provider install https://example.org/provider.zip
torii provider install aws
```

A resolução tenta path existente, URL HTTPS e, por último, nome no catálogo canônico. Archives ZIP, TAR e TAR.GZ/TGZ são aceitos. Extração limita tamanho e quantidade de arquivos e rejeita traversal, symlinks, hardlinks e arquivos especiais.

## Catálogo

O catálogo é um `index.yaml`:

```yaml
version: "1"
providers:
  - name: aws
    version: "0.1.0"
    description: Provider oficial AWS.
    source: releases/aws-0.1.0.zip
    sha256: 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef
```

`torii provider search [query]` lê por padrão `https://raw.githubusercontent.com/torii-mcp/torii-canon-providers/main/index.yaml`. Artefatos remotos exigem HTTPS e o SHA-256 do catálogo é conferido antes da extração. `TORII_PROVIDER_CATALOG` substitui o catálogo oficial por um path local ou URL HTTPS.

No catálogo oficial, `source` aponta para um asset ZIP de uma GitHub Release do repositório de providers. O fluxo de `provider install aws`, por exemplo, é:

1. baixar o `index.yaml` canônico;
2. localizar a entrada cujo `name` é `aws`;
3. resolver e baixar a URL HTTPS declarada em `source`;
4. comparar o SHA-256 do conteúdo com o valor do índice;
5. extrair em diretório temporário e validar manifest, provider, regras vazias e setups;
6. instalar o provider e registrar origem, versão e hash em `.torii-package/lock.yaml`.

O pacote é independente de Windows ou Linux porque contém apenas arquivos declarativos. O executável indicado por `provider.yaml`, como `aws` ou `kubectl`, continua sendo uma dependência externa instalada pelo usuário na plataforma.

## Estado instalado

```text
providers/aws/
├── provider.yaml            # gerenciado pelo pacote
├── rules.yaml               # propriedade do usuário
├── .env                     # propriedade do usuário
├── .torii-package/          # manifest, lock e setups instalados
├── grants
└── auth/
```

`install` cria rules vazio. `setup` é o único comando que escreve nele e só trabalha sobre uma política vazia. `update` substitui apenas `provider.yaml` e `.torii-package/`; não abre rules, ambiente ou estado operacional para escrita.
