# Catálogo de homologação live

Este documento define os casos de homologação humana do Torii. Ele é o catálogo estável; cada sessão real produz um arquivo de evidência separado em `tests/live/runs/YYYY-MM-DD.md`.

O catálogo precisa ser aprovado antes da primeira execução oficial. Uma execução exploratória não conta como evidência.

## Contrato da homologação

1. Toda operação enviada a AWS ou Kubernetes deve ser estritamente de leitura.
2. Cada caso começa em uma raiz nova `TORII_CONFIG_DIR` com prefixo `torii-live-<caso>-` sob o diretório temporário do sistema.
3. Instalam-se somente o provider alvo e, para testes com target, o provider estritamente necessário indicado no campo `provider`. Não se aplica nenhum setup de política.
4. O `rules.yaml` começa vazio e recebe somente as regras indispensáveis para aquele caso.
5. Antes de executar, o agente mostra no chat o conteúdo integral de cada arquivo de regras que será usado e aguarda aprovação explícita do operador.
6. Depois da aprovação, o agente informa os tokens da invocação e a ação humana exata: **Negar**, ou escolher **Temporariamente**, revisar/restaurar a sugestão estrutural ou selecionar `Exact`, definir a duração, marcar a confirmação de revisão e clicar **Permitir**.
7. O agente confere resposta MCP, arquivos locais permitidos e eventos sanitizados de `torii.log`.
8. Credenciais, conteúdo de `auth/credentials.env`, clipboard e stdout/stderr completos não entram na evidência.
9. Depois de registrar a evidência, o agente remove somente a raiz daquele caso, após validar que o caminho continua dentro do diretório temporário e possui o prefixo esperado.
10. Casos com múltiplas invocações preservam a mesma raiz até terminar todas as repetições. Isso se aplica a `allow once`, grant temporário, expiração, reautenticação transacional e isolamento entre targets.
11. Diante de qualquer tentativa de escrita remota, context produtivo, regra extra ou divergência entre o plano aprovado e o ambiente preparado, o caso para antes da chamada.
12. Os contexts reais dos casos Kubernetes são escolhidos somente no ambiente local e nunca entram no catálogo, evidência, resposta MCP ou auditoria. O target de referência é publicado apenas como `lab`; `K8S-AUTH-01` usa um segundo target não produtivo sob o alias `lab-noaccess`, com a mesma identidade válida do target de referência.

Escritas locais dentro da raiz isolada são esperadas: configuração, target, auditoria, cache, grants e sessão. Nenhum caso cria, altera ou remove recurso remoto.

## Quando a janela de autorização pode aparecer

A janela de autorização existe somente para uma decisão `unresolved`: a chamada não casou com `deny`, não casou com `accept` e não possui grant ativo.

- match em `deny`: encerra imediatamente como `explicit-deny`; não abre janela de autorização nem de autenticação e não inicia o provider;
- match em `accept`: segue como `allowed-by-rules`, sem janela de autorização; uma janela de autenticação ainda pode aparecer depois, caso a sessão do provider precise ser criada ou renovada;
- sem match em `deny` ou `accept`, mas com grant ativo: segue como `allowed-by-grant`, sem janela;
- sem regra e sem grant: abre a janela de autorização; em headless, assume **Negar** sem abrir GUI.

Portanto, qualquer janela exibida em um caso com deny explícito torna a instância `FAIL`, mesmo que o operador clique **Negar** e nenhum subprocesso seja iniciado.

## Gate obrigatório antes de cada caso

O agente apresenta um bloco com esta forma:

```text
Caso: <ID e nome>
Raiz isolada: <caminho>
Provider/target: <tool e alias>
Operações remotas: <lista completa; todas read-only>
Arquivos de regras: <conteúdo integral>
Invocações previstas: <quantidade e ordem>
Ações humanas: <cliques ou preenchimento esperado>
Eventos esperados: <eventos de auditoria>
Cleanup: <o que será removido>
```

Nenhuma invocação começa enquanto o operador não aprovar esse bloco. Alterar regra, comando, target ou sequência invalida a aprovação e exige um novo gate.

## Perfis mínimos de regras

Os casos abaixo referenciam estes perfis. Na execução, o YAML correspondente é mostrado novamente no chat.

### R0 — política vazia

```yaml
version: "1.0"
deny: []
accept: []
```

### RA1 — AWS aceita apenas identidade

```yaml
version: "1.0"
deny: []
accept:
  - "sts get-caller-identity"
```

### RA2 — AWS nega apenas identidade

```yaml
version: "1.0"
deny:
  - "sts get-caller-identity"
accept: []
```

### RA3 — AWS deny e accept concorrentes

```yaml
version: "1.0"
deny:
  - "sts get-caller-identity"
accept:
  - "sts get-caller-identity"
```

### RK1 — Kubernetes aceita apenas listagem de pods

```yaml
version: "1.0"
deny: []
accept:
  - "get pods"
```

### RK2 — Kubernetes nega apenas leitura de configuração

```yaml
version: "1.0"
deny:
  - "config view"
accept: []
```

### RK3 — Kubernetes deny e accept concorrentes

```yaml
version: "1.0"
deny:
  - "config view"
accept:
  - "config view"
```

Para providers target-aware, o gate identifica separadamente o `rules.yaml` compartilhado e o `targets/<alias>/rules.yaml`. Um arquivo não mencionado deve permanecer ausente ou exatamente em R0.

## Resumo dos casos

| ID | Área | Objetivo | Perfil | Interação humana |
|---|---|---|---|---|
| MCP-01 | MCP | discovery e schema das tools | R0 em ambos | nenhuma |
| AWS-AUTH-01 | autenticação | campos obrigatórios vazios | RA1 | Validar e usar; depois fechar/cancelar |
| AWS-AUTH-02 | autenticação | cancelamento sem sessão | RA1 | Cancelar |
| AWS-AUTH-03 | autenticação | candidata inválida não persiste | RA1 | credencial inválida; Validar e usar; Cancelar |
| AWS-AUTH-04 | autenticação | candidata válida é validada e usada | RA1 | credencial temporária válida; Validar e usar |
| AWS-AUTH-05 | autenticação | reauth inválido preserva sessão válida | RA1 | sessão válida; reauth inválido; Cancelar |
| AUTH-UI-01 | autenticação | layout mínimo com uma variável | RA1 em provider de teste | conferir layout; Cancelar |
| AUTH-UI-02 | autenticação | layout com quatro variáveis | RA1 em provider de teste | conferir layout; Cancelar |
| AUTHZ-UI-01 | autorização | layout normal e comando enorme | R0 | conferir estados e rolagem; Negar |
| AUTHZ-UI-02 | autorização | feedback de permitir uma vez | R0 | Permitir uma vez; Cancelar auth |
| AUTHZ-UI-03 | autorização | sugestão estrutural, expansão e feedback temporário | R0 | Temporariamente, prefixo sugerido, 2 min; Cancelar auth |
| AWS-POL-01 | Jasper | accept executa sem prompt de acesso | RA1 | autenticar se necessário |
| AWS-POL-02 | Jasper | deny explícito encerra antes de auth | RA2 | nenhuma |
| AWS-POL-03 | Jasper | deny vence accept | RA3 | nenhuma |
| AWS-POL-04 | Jasper | unresolved negado pelo humano | R0 | Negar |
| AWS-POL-05 | Jasper | allow once não persiste | R0 | Permitir uma vez; na repetição, Negar |
| AWS-POL-06 | Jasper | grant de 2 min, escopo e expiração | R0 | Temporariamente, Exact, 2 min; ao expirar, Negar |
| AWS-POL-07 | Jasper | headless mantém default deny | R0 | nenhuma |
| K8S-POL-01 | Jasper/target | accept no target selecionado | R0 compartilhado + RK1 no target | nenhuma |
| K8S-AUTH-01 | autenticação herdada | identidade válida sem acesso ao target de isolamento | R0 compartilhado + RK1 no target | nenhuma |
| K8S-POL-02 | Jasper/target | deny explícito de leitura | R0 compartilhado + RK2 no target | nenhuma |
| K8S-POL-03 | Jasper/target | deny vence accept | R0 compartilhado + RK3 no target | nenhuma |
| K8S-POL-04 | Jasper/target | unresolved negado pelo humano | R0 compartilhado e no target | Negar |
| K8S-POL-05 | Jasper/target | allow once não persiste | R0 compartilhado e no target | Permitir uma vez; na repetição, Negar |
| K8S-POL-06 | Jasper/target | grant exact, variação e expiração | R0 compartilhado e no target | Temporariamente, Exact, 2 min; negar variação e expiração |
| K8S-POL-07 | Jasper/target | grant prefix, variação e expiração | R0 compartilhado e no target | Temporariamente, prefixo `get pods`, 2 min; negar após expiração |
| K8S-TGT-01 | envelope | target obrigatório | R0 | nenhuma |
| K8S-TGT-02 | envelope | target desconhecido | R0 | nenhuma |
| K8S-TGT-03 | envelope | bloqueio de `--context` | R0 | nenhuma |
| K8S-TGT-04 | envelope | bloqueio de `--kubeconfig` | R0 | nenhuma |
| K8S-TGT-05 | envelope | bloqueio de `--server` | R0 | nenhuma |
| K8S-TGT-06 | envelope | bloqueio de `--token` | R0 | nenhuma |
| K8S-TGT-07 | política | regra do target substitui compartilhada | RK1 compartilhado + deny mínimo no target | nenhuma |
| K8S-TGT-08 | isolamento | grant de um alias não atravessa outro | R0 em dois targets | Temporariamente, Exact, 2 min em A; Negar em B |

## Casos MCP

### MCP-01 — discovery e schema

Preparo: raiz nova, providers AWS e Kubernetes instalados, um único target Kubernetes não produtivo chamado `lab`, ambos os arquivos compartilhados em R0 e sem arquivos de regras específicos no target.

Operação:

```powershell
cargo run --example mcp_probe -- $Torii $CaseRoot list
```

Critérios:

- existem exatamente as tools `aws` e `kubectl`;
- `aws` exige apenas `args`;
- `kubectl` exige `target` e `args`;
- o enum de `target` anuncia somente `lab`;
- nenhum context real aparece no schema;
- nenhuma operação de provider ou GUI ocorre;
- `torii.log` permanece ausente ou sem evento de invocação.

## Casos de autenticação AWS

Todos usam somente `sts get-caller-identity`, provider AWS e RA1. Eles não reutilizam credenciais de outro caso.

### AWS-AUTH-01 — campos obrigatórios vazios

1. Chamar `sts get-caller-identity`.
2. Confirmar que a altura da janela acompanha o formulário, sem espaço vertical excessivo e com campos e botões acessíveis.
3. Clicar **Validar e usar** sem preencher campos.
4. Encerrar a janela se ela permanecer aberta; se a invocação encerrar, não repetir.

Esperado: janela compacta e legível; erro indicando os três campos obrigatórios; nenhum `credentials.env`, nenhum `.session-cache`, nenhum `ran`. Auditoria contém `invoke`, `allowed-by-rules` e `session-invalid`.

### AWS-AUTH-02 — cancelar autenticação

1. Chamar `sts get-caller-identity`.
2. Clicar **Cancelar**.

Esperado: erro estruturado de autenticação cancelada; nenhum arquivo de sessão; nenhum `ran`.

### AWS-AUTH-03 — candidata inválida

1. Chamar `sts get-caller-identity`.
2. Preencher valores temporários deliberadamente inválidos, sem registrá-los na evidência.
3. Clicar **Validar e usar**.
4. Confirmar que a mesma janela permanece aberta, o formulário fica bloqueado e um indicador de progresso aparece durante a validação.
5. Confirmar que o indicador desaparece, o formulário é reabilitado e a mensagem de sessão recusada aparece na mesma janela; clicar **Cancelar**.

Esperado: comportamento visual descrito acima; `session-candidate-invalid`; nenhum `credentials.env` persistido; nenhum `.session-cache`; nenhum `ran` da operação solicitada.

### AWS-AUTH-04 — candidata válida

1. Chamar `sts get-caller-identity`.
2. Preencher credenciais temporárias válidas e clicar **Validar e usar**.

Esperado: a janela permanece aberta com o formulário bloqueado e indicador de progresso, fechando somente depois do sucesso; o comando interno de validação passa antes da persistência; a chamada solicitada termina com exit code zero; auditoria contém `session-refreshed` e `ran`; existência, tamanho, timestamp e hash dos arquivos de sessão podem ser registrados, nunca seu conteúdo.

### AWS-AUTH-05 — reauth inválido preserva sessão

1. Criar uma sessão válida dentro do próprio caso e registrar hash/timestamp de `credentials.env` sem abri-lo.
2. Executar `torii reauth aws` na mesma raiz.
3. Informar uma candidata inválida, clicar **Validar e usar**, observar a recusa e clicar **Cancelar**.
4. Conferir que hash/timestamp do arquivo anterior não mudaram.
5. Chamar `sts get-caller-identity` por MCP novamente.

Esperado: `reauth-forced` e `session-candidate-invalid`; sessão anterior intacta; chamada final com exit code zero.

### AUTH-UI-01 — formulário com uma variável

Usar uma cópia isolada do provider de teste com apenas um campo obrigatório e o mesmo comando read-only aceito por RA1. Abrir a autenticação, conferir que janela, barra de status e rodapé permanecem compactos e clicar **Cancelar** sem preencher ou validar.

Esperado: um único campo visível, sem região vazia excessiva ou scrollbar do formulário; nenhum validator, arquivo de sessão ou `ran`.

### AUTH-UI-02 — formulário com quatro variáveis

Usar uma cópia isolada do provider de teste com quatro campos obrigatórios e o mesmo comando read-only aceito por RA1. Abrir a autenticação, conferir acesso a todos os campos, barra de status e rodapé, e clicar **Cancelar** sem preencher ou validar.

Esperado: os quatro campos e o rodapé ficam acessíveis dentro dos limites da janela; se a cardinalidade exigir rolagem, ela pertence somente ao formulário; nenhum validator, arquivo de sessão ou `ran`.

## Casos de UI da autorização

Todos usam R0 e `sts get-caller-identity`. A janela mantém ações, resumo do escopo e status fixos no rodapé. A largura permanece estável; mudanças de modo ajustam somente a altura, preservando o centro atual. Cada decisão terminal permanece visível por um instante antes do fechamento automático.

### AUTHZ-UI-01 — layout e comando enorme

1. Abrir uma chamada normal, conferir os estados da barra ao marcar a confirmação e a duração e clicar **Negar**.
2. Em raiz nova, repetir com um argumento sintético longo que não altere a natureza read-only da operação.
3. Confirmar que a pílula longa mostra começo, fim e tamanho sem tooltip ilimitado; abrir a revisão paginada e percorrer ao menos duas páginas.
4. Alternar **Uma vez**, **Temporariamente**, `Exact` e prefixo; confirmar largura e centro estáveis, grupos fixo/variável legíveis e ações sempre acessíveis; clicar **Negar**.

Esperado: feedback `Acesso negado.` em coral; `invoke`, `denied-interface`; nenhuma autenticação, credencial, grant ou execução.

### AUTHZ-UI-02 — permitir uma vez

Manter **Uma vez** selecionado, marcar a confirmação de revisão e clicar **Permitir**. Confirmar `👍 Acesso autorizado uma vez.` em verde. Cancelar a autenticação que abrir em seguida.

Esperado: `invoke`, `override-once`, `session-invalid`; nenhum grant, credencial, cache ou `ran`.

### AUTHZ-UI-03 — permitir temporariamente

Escolher **Temporariamente**, confirmar que `sts get-caller-identity` foi sugerido antes da primeira flag, selecionar `2` minutos, marcar a confirmação de revisão e clicar **Permitir**. Confirmar `👍 Acesso autorizado por 2 min.` em verde. Cancelar a autenticação que abrir em seguida.

Esperado: `invoke`, `override-timed`, `session-invalid`; grant local presente antes do cleanup; nenhuma credencial, cache ou `ran`.

## Casos Jasper AWS

### AWS-POL-01 — accept por regra

Perfil RA1. Chamar `sts get-caller-identity`. Se a sessão ainda não existir dentro do caso, autenticar com candidata válida.

Esperado: nenhuma janela de autorização; `allowed-by-rules`; execução com exit code zero.

### AWS-POL-02 — deny explícito antes de auth

Perfil RA2. Chamar `sts get-caller-identity` sem preparar credenciais.

Esperado: `explicit-deny`; nenhuma janela de autorização ou autenticação; nenhum arquivo de autenticação/cache; nenhum subprocesso AWS. Se qualquer janela aparecer, o caso falha imediatamente.

### AWS-POL-03 — deny vence accept

Perfil RA3. Chamar `sts get-caller-identity` sem preparar credenciais.

Esperado: mesmo resultado de AWS-POL-02, com a regra negada identificada. A presença simultânea em `accept` não autoriza abrir janela.

### AWS-POL-04 — unresolved negado

Perfil R0. Chamar `sts get-caller-identity` e clicar **Negar**.

Esperado: `human-deny`; `denied-interface`; nenhum grant, auth, cache ou subprocesso AWS.

### AWS-POL-05 — permitir uma vez

Perfil R0.

1. Chamar `sts get-caller-identity`.
2. Manter **Uma vez** selecionado, marcar a confirmação de revisão e clicar **Permitir**.
3. Autenticar com candidata válida quando solicitado.
4. Repetir exatamente a mesma chamada.
5. Confirmar que a janela de autorização reaparece e clicar **Negar**.

Esperado: primeira decisão `human-once` com `override-once` e um único `ran`; nenhum arquivo de grant; segunda decisão `human-deny` sem segundo `ran`.

### AWS-POL-06 — grant temporário, escopo e expiração

Perfil R0.

1. Chamar `sts get-caller-identity --query Account`.
2. Escolher **Temporariamente**, selecionar **Somente esta invocação exata**, escolher `2` minutos, marcar a confirmação de revisão e clicar **Permitir**.
3. Autenticar quando solicitado.
4. Dentro da janela, chamar `sts get-caller-identity --output json`.
5. Confirmar que a GUI reaparece: um grant `Exact` não permite argumentos acrescentados. Clicar **Negar**.
6. Esperar a expiração confirmada pelo epoch do grant, em intervalos observáveis menores que 60 segundos.
7. Chamar novamente e clicar **Negar** quando a GUI reaparecer.

Esperado: `override-timed | 2min`; a repetição idêntica usa `allowed-by-grant` e `ran`; a variação e a chamada após expirar geram `denied-interface` e nenhum novo `ran`.

### AWS-POL-07 — unresolved em headless

Perfil R0, `TORII_NO_GUI=1`, chamada `sts get-caller-identity`.

Esperado: `human-deny`/`denied-interface`, sem janela, auth, grant ou execução.

## Casos Kubernetes

O context real padrão é escolhido localmente pelo operador e sempre exposto ao MCP apenas como `lab`. O target declara localmente `provider`, referenciando o provider instalado cujo lifecycle deve herdar. O conteúdo da credencial e os nomes reais dos contexts não entram no catálogo nem na evidência de cada instância; somente aliases, tools de provider, hashes permitidos e classificações de resultado. As rules desse provider permanecem em R0 quando o agente não precisa invocá-lo diretamente.

`K8S-AUTH-01` é a exceção controlada: usa um segundo context não produtivo sob o alias `lab-noaccess`, mas referencia o mesmo provider autenticador e usa uma identidade válida nele. O preflight do provider deve passar; a falta de acesso ao outro cluster é observada no exit code do `kubectl`, depois da autorização do Jasper.

### K8S-POL-01 — accept no target

Compartilhado R0; target `lab` RK1.

```text
get pods -n default -o name --request-timeout=10s
```

Esperado: `allowed-by-rules`, `preflight-provider`, sessão válida ou renovada no provider autenticador, `preflight-ok`, escopo `kubectl/lab`, exit code zero e nenhum context real na resposta/auditoria.

### K8S-AUTH-01 — identidade válida sem acesso ao outro target

Pré-condição: a mesma identidade usada em `K8S-POL-01` continua válida no provider autenticador. Raiz nova, provider alvo e provider autenticador, ambos com rules compartilhadas R0; target `lab-noaccess` RK1 e referência ao mesmo campo `provider`.

```text
get pods -n default -o name --request-timeout=10s
```

Esperado: `allowed-by-rules`, `preflight-provider`, sessão válida no provider autenticador, `preflight-ok`, um único `ran` com exit code diferente de zero e erro classificado como `Unauthorized` ou `Forbidden`; nenhuma GUI de autorização ou grant. Exit code zero torna o caso `FAIL`, pois demonstraria que a identidade de referência possui acesso de leitura ao target de isolamento. Falha de rede, DNS ou obtenção de token torna o caso inconclusivo, não um PASS de isolamento.

### K8S-POL-02 — deny explícito de leitura

Compartilhado R0; target `lab` RK2.

```text
config view --minify
```

Esperado: `explicit-deny`, nenhuma janela e nenhum subprocesso kubectl. Se qualquer janela aparecer, o caso falha imediatamente.

### K8S-POL-03 — deny vence accept

Compartilhado R0; target `lab` RK3; mesma chamada de K8S-POL-02.

Esperado: deny explícito vence, sem janela e sem subprocesso. A presença simultânea em `accept` não transforma a decisão em `unresolved`.

### K8S-POL-04 — unresolved negado

Compartilhado e target em R0.

```text
get namespaces --request-timeout=10s
```

Clicar **Negar**. Esperado: `human-deny`, nenhum grant e nenhum subprocesso kubectl.

### K8S-POL-05 — permitir uma vez

Compartilhado e target em R0.

1. Chamar `get namespaces --request-timeout=10s`.
2. Manter **Uma vez** selecionado, marcar a confirmação de revisão e clicar **Permitir**.
3. Repetir exatamente a chamada.
4. Confirmar que a GUI reaparece e clicar **Negar**.

Esperado: primeira chamada `override-once` e `ran`; nenhum grant; segunda chamada `denied-interface` sem segundo `ran`.

### K8S-POL-06 — grant exact, variação e expiração

Compartilhado e target em R0.

1. Chamar `get pods -n agente-financeiro --request-timeout=10s`.
2. Escolher **Temporariamente**, selecionar **Somente esta invocação exata**, escolher `2` minutos, marcar a confirmação de revisão e clicar **Permitir**.
3. Repetir exatamente a chamada dentro da janela; não deve abrir GUI.
4. Ainda dentro da janela, chamar `get pods -n agente-financeiro --request-timeout=10s -o name`; por ser grant `Exact`, deve abrir GUI. Clicar **Negar**.
5. Ainda dentro da janela, chamar `get pods -n agente-rm --request-timeout=10s`; por ser grant `Exact`, deve abrir GUI. Clicar **Negar**.
6. Depois da expiração, repetir a chamada original e clicar **Negar**.

Esperado: somente a repetição exata usa `allowed-by-grant`; a variação e a chamada expirada geram `denied-interface`.

### K8S-POL-07 — grant prefix, variação e expiração

Compartilhado e target em R0, numa raiz nova para não reutilizar o grant anterior.

1. Chamar `get pods -n agente-financeiro --request-timeout=10s`.
2. Escolher **Temporariamente**, confirmar a sugestão `get pods` antes de `-n`, escolher `2` minutos, marcar a confirmação de revisão e clicar **Permitir**.
3. Dentro da janela, chamar `get pods -n agente-rm --request-timeout=10s`; não deve abrir GUI.
4. Ainda dentro da janela, chamar `get pods`; não deve abrir GUI.
5. Depois da expiração, chamar `get pods -n agente-financeiro --request-timeout=10s` e clicar **Negar**.

Esperado: as chamadas cuja sequência começa por `get pods` usam `allowed-by-grant`; a chamada após expirar gera `denied-interface`. A evidência registra apenas o modo e a contagem de tokens do grant, nunca o conteúdo de argumentos ou credenciais.

## Casos de fronteira do target

Todos usam somente o verbo read-only `get pods`. Como a rejeição deve ocorrer antes da política, os arquivos ficam em R0 e o log não deve ganhar `invoke`.

### K8S-TGT-01 — target ausente

Chamar a tool `kubectl` sem o campo `target`. Esperado: erro de argumentos informando que target é obrigatório.

### K8S-TGT-02 — target desconhecido

Com somente `lab` cadastrado, chamar com `target: inexistente`. Esperado: erro que anuncia apenas `lab` como disponível.

### K8S-TGT-03 — `--context`

```text
get pods --context outro
```

Esperado: opção locked pelo target, sem subprocesso.

### K8S-TGT-04 — `--kubeconfig`

```text
get pods --kubeconfig arquivo-ficticio
```

Esperado: opção locked, sem leitura do arquivo e sem subprocesso.

### K8S-TGT-05 — `--server=...`

```text
get pods --server=https://invalid.example
```

Esperado: opção locked, sem tentativa de rede.

### K8S-TGT-06 — `--token=...`

```text
get pods --token=valor-ficticio
```

Esperado: opção locked. O valor fictício não deve aparecer na auditoria, que deve permanecer inalterada.

### K8S-TGT-07 — política do target substitui compartilhada

Compartilhado RK1. No target `lab`, usar:

```yaml
version: "1.0"
deny:
  - "get pods"
accept: []
```

Chamar `get pods -n default --request-timeout=10s`.

Esperado: deny explícito do target; a regra compartilhada não é combinada nem usada; nenhum subprocesso.

### K8S-TGT-08 — grant isolado por alias

Dois aliases, `lab_a` e `lab_b`, apontam para o mesmo context não produtivo aprovado. Compartilhado e ambos os targets em R0.

1. Em `lab_a`, chamar `get namespaces --request-timeout=10s` e permitir por 2 minutos.
2. Repetir em `lab_a`; deve usar o grant sem GUI.
3. Dentro da mesma janela, chamar exatamente os mesmos args em `lab_b`.
4. Confirmar que a GUI aparece e clicar **Negar**.

Esperado: grant existe somente sob `lab_a`; auditoria separa `kubectl/lab_a` e `kubectl/lab_b`.

## Evidência por instância

Depois da aprovação deste catálogo, a sessão cria `tests/live/runs/2026-07-15.md`. Cada caso recebe uma instância antes do preparo, por exemplo `LIVE-2026-07-15-001`.

Cada instância registra:

- ID da instância e ID do caso;
- revisão do catálogo e commit/base do código;
- horário inicial e final;
- versões de Torii, AWS CLI e kubectl;
- raiz temporária e alias, sem context real;
- conteúdo integral dos rules aprovados e seus hashes;
- sequência de invocações e ações humanas solicitadas;
- decisão, origem, regra, exit code e presença de GUI observados;
- eventos relevantes e sanitizados de auditoria;
- presença, tamanho, hash e timestamp de grants/cache/auth quando necessários, nunca o conteúdo de credenciais;
- resultado `PASS`, `FAIL`, `BLOCKED` ou `NOT RUN`;
- bug ou divergência encontrados;
- confirmação de cleanup ou motivo aprovado para preservar a raiz.

O resumo do dia lista todas as instâncias planejadas. Um caso não executado continua explícito como `NOT RUN`; ele não desaparece da evidência.

## Critério de aprovação do catálogo

Ao aprovar, o operador pode:

- aprovar todos os casos;
- excluir casos por ID;
- alterar a ordem;
- exigir uma pausa entre grupos;
- pedir mudança em comandos, rules ou evidência.

Qualquer mudança posterior neste arquivo invalida somente as instâncias ainda não executadas e exige nova aprovação para elas.
