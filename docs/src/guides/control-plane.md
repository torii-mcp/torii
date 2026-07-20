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

Reauth aplica-se a autenticação gerenciada; `inherited` sem validator não possui material renovável pelo Torii. O reauth de um target delega ao lifecycle do provider de identidade (`identity.provider`), no balde do escopo do target. Um provider `inherited` com validator (login externo via SSO/profile) não é renovado por `reauth`: o humano autentica pelo fluxo nativo e o agente repete o alias.

## Targets

```powershell
torii target add kubectl meu_dev --context contexto-local --provider aws
torii target add aws_profile producao --profile empresa-producao --account-id 111122223333 --region sa-east-1
torii target list kubectl
torii target show kubectl meu_dev
torii target activate kubectl meu_dev --for 30
torii target status kubectl
torii target activate aws_profile homologacao --for 30 --add
torii target clear aws_profile
torii target remove kubectl meu_dev --force
```

O primeiro comando cria um binding Kubernetes; o segundo cria um binding AWS de profile e conta esperada. Profile e conta aparecem somente nos comandos humanos `target list` e `target show`, não no MCP. Criar não ativa: todos os aliases target-aware começam sem lease, mas continuam anunciados no schema MCP.

`target activate` libera temporariamente um alias. Sem `--add`, ele substitui todos os aliases ativos daquela tool; com `--add`, preserva os existentes. A duração aceita 1 a 1.440 minutos e usa `default_target_minutes` (15) quando `--for` não é informado. Ao manter mais de um ativo, aceite deliberadamente que o agente poderá escolher qualquer alias ativo em uma operação permitida. `target status` mostra os leases e expirações. `target clear` revoga somente leases: não remove target, rules, grants, `.env`, cache ou credenciais e não encerra processos já iniciados.

Reinicie o servidor MCP após install, update ou mudança no conjunto de targets. A criação/remoção muda o enum do schema; ativar, limpar ou aguardar a expiração não muda o enum. `rules.yaml` e o estado de lease são relidos durante cada chamada.
