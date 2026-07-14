# Operar providers e sessões

O control plane é humano. Nenhum comando desta página aparece no MCP.

## Catálogo e providers locais

```powershell
torii provider search
torii provider search kubernetes
torii provider list
```

Search consulta o catálogo; list inspeciona apenas instalações locais e mostra tool, nome, executável, versão e origem.

## Instalar

```powershell
torii provider install aws
torii provider install ./pacotes/aws
torii provider install ./pacotes/aws.zip
torii provider install https://example.org/aws.tar.gz
```

O install valida manifest, provider e política base; extrai em staging e faz rename. Um destino existente é recusado. O `rules.yaml` ativo nasce vazio.

## Aplicar setup

```powershell
torii provider setup aws readonly
```

Pacotes podem oferecer vários setups read-only. Setup é o único comando que escreve em rules e recusa substituir uma política que já tenha accepts ou denies.

## Atualizar

```powershell
torii provider update aws
```

Update usa a origem gravada no lock. Ele substitui `provider.yaml` e metadados/setups do pacote. Rules ativo, `.env`, grants, targets, cache e autenticação não são abertos para escrita.

## Inicializar e descobrir paths

```powershell
torii init
torii config-dir
```

Init cria somente settings e raiz. Providers são sempre uma escolha explícita.

## Reautenticar

```powershell
torii reauth aws
torii reauth kubectl meu_dev
```

Reauth aplica-se a autenticação gerenciada; `inherited` não possui material renovável pelo Torii.

## Targets

```powershell
torii target add kubectl meu_dev --context eks-meu-dev
torii target list kubectl
torii target show kubectl meu_dev
torii target remove kubectl meu_dev --force
```

Reinicie o servidor MCP após install, update ou mudança no conjunto de targets. `rules.yaml` é recarregado em cada chamada.
