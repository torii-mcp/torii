# Evidência live — Kubernetes — 2026-07-15

Status da sessão: `CLOSED`

Esta sessão valida somente operações Kubernetes de leitura. Contexts e identificadores reais de cliente permanecem exclusivamente na configuração temporária local e nunca entram nesta evidência.

## Contrato da credencial-base

- uma única candidata AWS temporária será coletada pela GUI e validada com `sts get-caller-identity`;
- somente `auth/credentials.env` será copiado para um diretório dedicado sob o TEMP;
- `.session-cache` não será reutilizado: cada caso que exigir identidade válida executará o validator do provider sem nova coleta humana;
- casos de deny explícito ou envelope inválido não receberão a cópia;
- conteúdo, identidade, stdout e hashes da credencial não serão registrados;
- a cópia-base e todas as raízes de caso serão removidas ao final da suíte.

## Resumo

| Instância | Caso | Resultado | Observação |
|---|---|---|---|
| LIVE-2026-07-15-K00 | K8S-BOOTSTRAP | PASS | credencial-base validada, copiada sem leitura e raiz limpa |
| LIVE-2026-07-15-K00-R1 | K8S-BOOTSTRAP | PASS | credencial-base correta validada e substituída atomicamente |
| LIVE-2026-07-15-K01 | K8S-POL-01 | FAIL | lifecycle/preflight passaram; kubectl retornou exit code 1 |
| LIVE-2026-07-15-K01-R1 | K8S-POL-01 | FAIL | API server classificou a identidade como unauthorized |
| LIVE-2026-07-15-K01-R2 | K8S-POL-01 | PASS | accept e execução read-only concluídos com a credencial correta |
| LIVE-2026-07-15-K02 | K8S-AUTH-01 | PASS | provider válido e API server recusou o target isolado |
| LIVE-2026-07-15-K03 | K8S-POL-02 | PASS | deny explícito encerrou antes de autenticação e provider |
| LIVE-2026-07-15-K04 | K8S-POL-03 | PASS | deny venceu accept antes de autenticação e provider |
| LIVE-2026-07-15-K05 | K8S-POL-04 | PASS | unresolved negado antes de lifecycle e provider |
| LIVE-2026-07-15-K06 | K8S-POL-05 | PASS | allow once não persistiu e segunda chamada foi negada |
| LIVE-2026-07-15-K07 | K8S-POL-06 | PASS | grant exact funcionou, não cobriu variação e expirou |
| LIVE-2026-07-15-K08 | K8S-TGT-01 | PASS | target ausente foi rejeitado no envelope, sem efeitos posteriores |
| LIVE-2026-07-15-K09 | K8S-TGT-02 | PASS | target desconhecido foi rejeitado no envelope, sem efeitos posteriores |
| LIVE-2026-07-15-K10 | K8S-TGT-03 | PASS | `--context` foi bloqueado pelo target antes da política |
| LIVE-2026-07-15-K11 | K8S-TGT-04 | PASS | `--kubeconfig` foi bloqueado antes de qualquer leitura |
| LIVE-2026-07-15-K12 | K8S-TGT-05 | PASS | `--server` foi bloqueado antes de qualquer tentativa de rede |
| LIVE-2026-07-15-K13 | K8S-TGT-06 | PASS | `--token` sintético foi bloqueado antes de credenciais e execução |
| LIVE-2026-07-15-K14 | K8S-TGT-07 | PASS | rules do target substituíram o accept compartilhado com deny explícito |
| LIVE-2026-07-15-K15 | K8S-TGT-08 | PASS | grant temporário permaneceu isolado no alias de origem |

## LIVE-2026-07-15-K00 — K8S-BOOTSTRAP

- início do preparo: `2026-07-15T18:51:18-03:00`;
- raiz isolada: `C:\Users\paulo\AppData\Local\Temp\torii-live-K8S-BOOTSTRAP-20260715-001`;
- binário Torii SHA-256: `81adaa0cf25ea406899cdd88ccf6510364a771bbbf48ca35ebe0906e1ec3a85d`;
- provider AWS: `0.1.1`;
- AWS CLI: `2.31.13`;
- operação solicitada e validator: `sts get-caller-identity`, estritamente read-only;
- estado inicial: sem credenciais, cache, grants ou auditoria;
- resultado: `PASS`.

Rules aprovadas, SHA-256 `6b82372870e90024ab37209bdf403c118b164f1f89938d196f4ae72919320913`:

```yaml
version: "1.0"
deny: []
accept:
  - "sts get-caller-identity"
```

Ação humana prevista: fornecer uma candidata temporária válida somente na GUI, clicar **Validar e usar** e aguardar o fechamento automático após o status verde.

Esperado: `allowed-by-rules`, `session-invalid`, `session-refreshed`, um `ran`; credencial persistida apenas na raiz isolada até a cópia-base; nenhuma identidade ou saída retida.

Resultados observados:

- operador forneceu a candidata somente na GUI e confirmou o sucesso;
- a saída da operação foi descartada integralmente pelo harness;
- eventos, em ordem: `invoke`, `allowed-by-rules`, `session-invalid`, `session-refreshed`, `ran`;
- `ran`: um, exit code zero;
- credencial validada: `1143` bytes; conteúdo e hash não registrados;
- cópia-base: criada byte a byte no diretório TEMP dedicado `torii-live-auth-base-20260715-k8s`;
- `.session-cache`: não foi copiado;
- grants: ausentes;
- processos `torii` e `mcp_probe` remanescentes: zero;
- fim: `2026-07-15T18:53:41-03:00`;
- cleanup: `PASS`; a raiz de bootstrap não existe mais e somente a cópia-base controlada permanece até o encerramento da suíte.

## LIVE-2026-07-15-K00-R1 — K8S-BOOTSTRAP

- caso: substituição da credencial-base informada incorretamente pelo operador;
- início do preparo: `2026-07-15T19:11:03-03:00`;
- raiz isolada: `C:\Users\paulo\AppData\Local\Temp\torii-live-K8S-BOOTSTRAP-REAUTH-20260715-002`;
- provider AWS: `0.1.1`;
- operação remota: somente validator `sts get-caller-identity`, estritamente read-only;
- rules do provider: R0, SHA-256 `a9fcb303e907187487420f83932db545f81c40184a85ee179867facc660d8758`;
- credencial-base anterior: preservada até a candidata ser validada;
- candidata e cache: ausentes antes da abertura;
- resultado: `PASS`.

Ação humana prevista: informar a credencial correta somente na GUI e clicar **Validar e usar**.

Esperado: candidata validada antes da persistência; substituição byte a byte da cópia-base somente após sucesso; conteúdo e hashes não registrados; cleanup integral da raiz de reautenticação.

Resultados observados:

- operador informou a candidata correta somente na GUI;
- reautenticação: exit code zero;
- eventos, em ordem: `reauth-forced`, `session-refreshed`;
- a candidata validada difere da base anterior;
- substituição da cópia-base: atômica e verificada byte a byte;
- arquivo final: `1143` bytes; conteúdo e hashes não registrados;
- candidato de staging e backup anterior: removidos;
- processos `torii` e `mcp_probe` remanescentes: zero;
- fim: `2026-07-15T19:12:55-03:00`;
- cleanup: `PASS`; raiz de reautenticação removida e somente a nova credencial-base permanece controlada.

## LIVE-2026-07-15-K01 — K8S-POL-01

- caso: accept mínimo no target selecionado;
- início do preparo: `2026-07-15T18:56:00-03:00`;
- raiz isolada: `C:\Users\paulo\AppData\Local\Temp\torii-live-K8S-POL-01-20260715-001`;
- target público: `lab`; context real não registrado;
- lifecycle provider: `aws`;
- operações: validator `sts get-caller-identity` e `kubectl get pods --request-timeout=10s`, ambas estritamente read-only;
- credencial-base: copiada byte a byte sem leitura de conteúdo;
- `.session-cache`: ausente antes da chamada;
- grants e auditoria: ausentes antes da chamada;
- resultado: `FAIL`.

`providers/aws/rules.yaml`, SHA-256 `a9fcb303e907187487420f83932db545f81c40184a85ee179867facc660d8758`:

```yaml
version: "1.0"
deny: []
accept: []
```

`providers/kubectl/rules.yaml`, SHA-256 `a9fcb303e907187487420f83932db545f81c40184a85ee179867facc660d8758`:

```yaml
version: "1.0"
deny: []
accept: []
```

`providers/kubectl/targets/lab/rules.yaml`, SHA-256 `e938abc1cbb0cce0a2cc7a10f20c2592868fced52402b437e7a35cd0aa6c4f49`:

```yaml
version: "1.0"
deny: []
accept:
  - "get pods"
```

Ação humana prevista: nenhuma.

Esperado: nenhuma GUI; `allowed-by-rules`, lifecycle AWS validado, preflight aprovado e um único `ran` do kubectl com exit code zero; stdout não retido; cleanup integral da raiz, preservando somente a credencial-base.

Resultados observados:

- nenhuma GUI apareceu e os processos encerraram automaticamente;
- eventos, em ordem: `invoke`, `allowed-by-rules`, `preflight-provider`, `session-ok`, `preflight-ok`, `ran`;
- lifecycle AWS e preflight: sucesso;
- `ran`: um, exit code `1`; critério de exit code zero: `FAIL`;
- stdout e stderr foram descartados integralmente pelo harness, portanto a causa não foi classificada nesta instância;
- credencial e cache existiram somente dentro da raiz do caso; grants ausentes;
- processos `torii` e `mcp_probe` remanescentes: zero;
- fim: `2026-07-15T18:56:52-03:00`;
- follow-up: retry em raiz nova com probe de resumo seguro, sem emitir stdout/stderr completos;
- cleanup: `PASS`; a raiz isolada não existe mais e a credencial-base permanece controlada.

## LIVE-2026-07-15-K01-R1 — K8S-POL-01

- caso: retry diagnóstico do accept mínimo no target;
- início do preparo: `2026-07-15T19:01:13-03:00`;
- raiz isolada: `C:\Users\paulo\AppData\Local\Temp\torii-live-K8S-POL-01-20260715-001-r1`;
- target público: `lab`; context real não registrado;
- lifecycle provider: `aws`;
- operação: `kubectl get pods --request-timeout=10s`, estritamente read-only;
- rules: idênticas às aprovadas em K01;
- credencial-base: copiada byte a byte, sem `.session-cache`;
- saída completa: autorizada pelo operador somente em arquivo local dentro da raiz do caso; conteúdo não entra nesta evidência;
- resultado: `FAIL`.

Ação humana prevista: inspecionar o arquivo de diagnóstico aberto no VS Code e classificar a causa; nenhuma interação com Torii.

Esperado: mesma cadeia de lifecycle/preflight; diagnóstico local suficiente para classificar o exit code; arquivo removido junto com a raiz depois da inspeção.

Resultados observados:

- processo monitorado até o encerramento; nenhuma GUI;
- eventos, em ordem: `invoke`, `allowed-by-rules`, `preflight-provider`, `session-ok`, `preflight-ok`, `ran`;
- `ran`: um, exit code `1`;
- operador classificou localmente o stderr como `unauthorized`; texto completo não foi copiado para a evidência;
- inspeção estrutural do kubeconfig: exec plugin AWS/EKS, sem `--profile`, sem `AWS_PROFILE`, sem credenciais próprias e sem role forçada;
- inferência: a credencial chega ao exec plugin, mas a identidade válida no provider não é aceita pelo API server do target selecionado;
- arquivo diagnóstico: `2662` bytes, removido sem retenção de conteúdo;
- credencial e cache existiram somente dentro da raiz; grants ausentes;
- processos `torii` e `mcp_probe` remanescentes: zero;
- fim: `2026-07-15T19:03:30-03:00`;
- cleanup: `PASS`; arquivo diagnóstico e raiz isolada não existem mais; credencial-base permanece controlada.

## LIVE-2026-07-15-K01-R2 — K8S-POL-01

- caso: retry do accept mínimo após correção da credencial-base;
- início do preparo: `2026-07-15T19:14:44-03:00`;
- raiz isolada: `C:\Users\paulo\AppData\Local\Temp\torii-live-K8S-POL-01-20260715-001-r2`;
- target público: `lab`; seleção local exigiu exatamente um context explicitamente marcado como desenvolvimento; nome real não registrado;
- lifecycle provider: `aws`;
- operação: `kubectl get pods --request-timeout=10s`, estritamente read-only;
- rules e hashes: idênticos aos aprovados em K01;
- nova credencial-base: copiada byte a byte, sem `.session-cache`;
- probe: modo de resumo seguro, sem stdout/stderr completos;
- resultado: `PASS`.

Ação humana prevista: nenhuma.

Esperado: nenhuma GUI; lifecycle AWS validado, preflight aprovado e um `ran` com exit code zero; resumo informa somente presença/tamanho das saídas, nunca seu conteúdo.

Resultados observados:

- processo monitorado até o encerramento; nenhuma GUI;
- resumo seguro: decisão `allow` por `rules`, exit code zero, stdout presente com `213` bytes, stderr ausente, sem truncamento;
- conteúdo do stdout não foi exibido nem retido;
- eventos, em ordem: `invoke`, `allowed-by-rules`, `preflight-provider`, `session-ok`, `preflight-ok`, `ran`;
- `ran`: um, exit code zero;
- credencial e cache existiram somente dentro da raiz; grants ausentes;
- processos `torii` e `mcp_probe` remanescentes: zero;
- fim: `2026-07-15T19:16:01-03:00`;
- cleanup: `PASS`; raiz isolada removida e credencial-base correta preservada.

## LIVE-2026-07-15-K10 — K8S-TGT-03

- caso: bloqueio de `--context` pelo target;
- início do preparo: `2026-07-15T19:49:57-03:00`;
- raiz isolada: `C:\Users\paulo\AppData\Local\Temp\torii-live-K8S-TGT-03-20260715-010`;
- target público: `lab`; context real não registrado;
- argumento sintético: `get pods --context outro`; operação base estritamente read-only;
- AWS, kubectl compartilhado e target em R0 explícito, SHA-256 `a9fcb303e907187487420f83932db545f81c40184a85ee179867facc660d8758`;
- credencial-base: deliberadamente não copiada;
- resultado: `PASS`.

Rules aprovadas em todos os três escopos:

```yaml
version: "1.0"
deny: []
accept: []
```

Ação humana prevista: nenhuma.

Esperado: opção locked rejeitada antes da política; nenhuma GUI, autenticação, leitura de credencial, preflight, auditoria ou execução.

Resultados observados:

- resumo seguro: `is_error: true`, classe `blocked-option`, decisão e execução nulas;
- credenciais, cache e grants: zero antes e depois;
- `torii.log`: ausente, confirmando zero eventos `invoke` e `ran`;
- processos `torii` e `mcp_probe` remanescentes: zero;
- fim: `2026-07-15T19:50:08-03:00`;
- cleanup: `PASS`; raiz isolada removida com caminho validado; credencial-base preservada.

## LIVE-2026-07-15-K08 — K8S-TGT-01

- caso: target obrigatório;
- início do preparo: `2026-07-15T19:46:00-03:00`;
- raiz isolada: `C:\Users\paulo\AppData\Local\Temp\torii-live-K8S-TGT-01-20260715-008`;
- target público cadastrado: `lab`; context real não registrado;
- operação solicitada no envelope: `kubectl get pods`, estritamente read-only, sem informar `target`;
- AWS, kubectl compartilhado e target em R0 explícito, SHA-256 `a9fcb303e907187487420f83932db545f81c40184a85ee179867facc660d8758`;
- credencial-base: deliberadamente não copiada;
- resultado: `PASS`.

Rules aprovadas em todos os três escopos:

```yaml
version: "1.0"
deny: []
accept: []
```

Ação humana prevista: nenhuma.

Esperado: erro estruturado de target obrigatório antes da política; nenhuma GUI, autenticação, leitura de credencial, preflight, auditoria ou execução.

Resultados observados:

- resumo seguro: `is_error: true`, classe `target-required`, decisão e execução nulas;
- credenciais, cache e grants: zero antes e depois;
- `torii.log`: ausente, confirmando zero eventos `invoke` e `ran`;
- processos `torii` e `mcp_probe` remanescentes: zero;
- fim: `2026-07-15T19:46:31-03:00`;
- cleanup: `PASS`; raiz isolada removida com caminho validado; credencial-base preservada.

## LIVE-2026-07-15-K09 — K8S-TGT-02

- caso: target desconhecido;
- início do preparo: `2026-07-15T19:48:04-03:00`;
- raiz isolada: `C:\Users\paulo\AppData\Local\Temp\torii-live-K8S-TGT-02-20260715-009`;
- único target público cadastrado: `lab`; context real não registrado;
- operação solicitada no envelope: `kubectl get pods`, estritamente read-only, com alias fictício `inexistente`;
- AWS, kubectl compartilhado e target cadastrado em R0 explícito, SHA-256 `a9fcb303e907187487420f83932db545f81c40184a85ee179867facc660d8758`;
- credencial-base: deliberadamente não copiada;
- resultado: `PASS`.

Rules aprovadas em todos os três escopos:

```yaml
version: "1.0"
deny: []
accept: []
```

Ação humana prevista: nenhuma.

Esperado: erro estruturado de target desconhecido antes da política; nenhuma GUI, autenticação, leitura de credencial, preflight, auditoria ou execução.

Resultados observados:

- resumo seguro: `is_error: true`, classe `unknown-target`, decisão e execução nulas;
- credenciais, cache e grants: zero antes e depois;
- `torii.log`: ausente, confirmando zero eventos `invoke` e `ran`;
- processos `torii` e `mcp_probe` remanescentes: zero;
- fim: `2026-07-15T19:48:15-03:00`;
- cleanup: `PASS`; raiz isolada removida com caminho validado; credencial-base preservada.

## LIVE-2026-07-15-K03 — K8S-POL-02

- caso: deny explícito de leitura de configuração;
- início do preparo: `2026-07-15T19:20:30-03:00`;
- raiz isolada: `C:\Users\paulo\AppData\Local\Temp\torii-live-K8S-POL-02-20260715-003`;
- target público: `lab`; context real não registrado;
- operação solicitada: `kubectl config view`, read-only;
- credencial-base: deliberadamente não copiada;
- credencial, cache, grants e auditoria: ausentes antes da chamada;
- resultado: `PASS`.

`providers/aws/rules.yaml` e `providers/kubectl/rules.yaml`: R0, SHA-256 `a9fcb303e907187487420f83932db545f81c40184a85ee179867facc660d8758`.

`providers/kubectl/targets/lab/rules.yaml`, SHA-256 `4ee3179a59546a8472463b1484a0418d5dfbc9701f993aa7e6e0c0b3dcdb7d73`:

```yaml
version: "1.0"
deny:
  - "config view"
accept: []
```

Ação humana prevista: nenhuma.

Esperado: decisão `deny` por `explicit-deny`; nenhuma GUI, preflight, leitura de credencial, cache, grant ou `ran`.

Resultados observados:

- resumo seguro: decisão `deny` por `explicit-deny`, execução nula;
- nenhuma GUI;
- eventos, em ordem: `invoke`, `denied-explicit`;
- preflight e `ran`: zero;
- credencial, cache e grants: ausentes;
- processos `torii` e `mcp_probe` remanescentes: zero;
- fim: `2026-07-15T19:21:10-03:00`;
- cleanup: `PASS`; raiz isolada removida e credencial-base preservada fora do caso.

## LIVE-2026-07-15-K04 — K8S-POL-03

- caso: deny vence accept concorrente;
- início do preparo: `2026-07-15T19:23:10-03:00`;
- raiz isolada: `C:\Users\paulo\AppData\Local\Temp\torii-live-K8S-POL-03-20260715-004`;
- target público: `lab`; context real não registrado;
- operação solicitada: `kubectl config view`, read-only;
- credencial-base: deliberadamente não copiada;
- credencial, cache, grants e auditoria: ausentes antes da chamada;
- resultado: `PASS`.

Rules compartilhadas AWS/kubectl: R0.

`providers/kubectl/targets/lab/rules.yaml`, SHA-256 `3a7b9b17e8a4f2ed8d34742412637706e609a849e3f9225410dcdd14fb7118ca`:

```yaml
version: "1.0"
deny:
  - "config view"
accept:
  - "config view"
```

Ação humana prevista: nenhuma.

Esperado: `explicit-deny`; nenhuma GUI, preflight, credencial, cache, grant ou `ran`.

Resultados observados:

- resumo seguro: `deny` por `explicit-deny`, execução nula;
- nenhuma GUI;
- eventos: `invoke`, `denied-explicit`;
- preflight e `ran`: zero;
- credencial, cache e grants: ausentes;
- processos `torii` e `mcp_probe` remanescentes: zero;
- fim: `2026-07-15T19:23:50-03:00`;
- cleanup: `PASS`; raiz isolada removida e credencial-base preservada.

## LIVE-2026-07-15-K06 — K8S-POL-05

- caso: permitir uma vez não persiste;
- início do preparo: `2026-07-15T19:30:26-03:00`;
- raiz isolada: `C:\Users\paulo\AppData\Local\Temp\torii-live-K8S-POL-05-20260715-006`;
- target público: `lab`; context real não registrado;
- operação nas duas invocações: `kubectl get pods --request-timeout=10s`, estritamente read-only;
- três arquivos de rules em R0 explícito, SHA-256 `a9fcb303e907187487420f83932db545f81c40184a85ee179867facc660d8758`;
- credencial-base: copiada byte a byte, sem `.session-cache`;
- grants e auditoria: ausentes antes da primeira chamada;
- resultado: `PASS`.

Rules aprovadas:

```yaml
version: "1.0"
deny: []
accept: []
```

Ações humanas previstas: primeira janela, marcar ciência e **Permitir** sem duração; segunda janela, **Negar**.

Esperado: primeira decisão `human-once`, lifecycle/preflight e um `ran` com exit zero; nenhum grant; segunda autorização reaparece e produz `human-deny` sem segundo `ran`.

Resultados observados:

- primeira invocação: operador permitiu uma vez; resumo `allow` por `human-once`, exit code zero, stdout `213` bytes não exibido e stderr ausente;
- depois da primeira invocação: um `ran`, cache de sessão presente e grant ausente;
- segunda invocação idêntica: janela reapareceu e operador negou; resumo `deny` por `human-deny`, execução nula;
- eventos, em ordem: `invoke`, `override-once`, `preflight-provider`, `session-ok`, `preflight-ok`, `ran`, `invoke`, `denied-interface`;
- invocações: duas; `ran`: exatamente um;
- grant: ausente durante todo o caso;
- processos `torii` e `mcp_probe` remanescentes: zero;
- fim: `2026-07-15T19:33:59-03:00`;
- cleanup: `PASS`; raiz isolada, credencial e cache do caso removidos; credencial-base preservada.

## LIVE-2026-07-15-K07 — K8S-POL-06

- caso: grant temporário exact, variação e expiração;
- início do preparo: `2026-07-15T19:36:00-03:00`;
- raiz isolada: `C:\Users\paulo\AppData\Local\Temp\torii-live-K8S-POL-06-20260715-007`;
- target público: `lab`; context real não registrado;
- operação original: `kubectl get pods -o name --request-timeout=10s`;
- variação: `kubectl get namespaces -o name --request-timeout=10s`;
- todas as operações são estritamente read-only;
- três arquivos de rules em R0 explícito, SHA-256 `a9fcb303e907187487420f83932db545f81c40184a85ee179867facc660d8758`;
- credencial-base: copiada byte a byte, sem `.session-cache`;
- grants e auditoria: ausentes antes da primeira chamada;
- resultado: `PASS`.

Ações humanas previstas: permitir a chamada original por `2` minutos; negar a variação; após expirar, negar a chamada original.

Esperado: primeira chamada `human-grant` e `ran`; repetição exata `allowed-by-grant` e `ran`; variação e chamada expirada negadas; exatamente dois `ran`; cleanup integral ao final.

Resultados observados:

- primeira invocação: `allow` por `human-grant`, exit zero, stdout `75` bytes não exibido; grant de `2` minutos criado com regra exatamente igual ao vetor completo;
- repetição exata antes da expiração: `allow` por `grant`, exit zero, sem GUI;
- variação para namespaces: autorização abriu e operador negou, sem terceiro `ran`;
- expiração confirmada pelo epoch local antes da quarta invocação;
- chamada original após expirar: autorização reapareceu e operador negou, sem novo `ran`;
- eventos, em ordem: `invoke`, `override-timed`, `preflight-provider`, `session-ok`, `preflight-ok`, `ran`, `invoke`, `allowed-by-grant`, `preflight-provider`, `preflight-ok`, `ran`, `invoke`, `denied-interface`, `invoke`, `denied-interface`;
- totais: quatro invocações, um `override-timed`, um `allowed-by-grant`, duas negações e exatamente dois `ran` com exit zero;
- processos `torii` e `mcp_probe` remanescentes: zero;
- fim: `2026-07-15T19:42:01-03:00`;
- cleanup: `PASS`; raiz, grant expirado, credencial e cache do caso removidos; credencial-base preservada.

## LIVE-2026-07-15-K05 — K8S-POL-04

- caso: unresolved negado pelo operador;
- início do preparo: `2026-07-15T19:24:46-03:00`;
- raiz isolada: `C:\Users\paulo\AppData\Local\Temp\torii-live-K8S-POL-04-20260715-005`;
- target público: `lab`; context real não registrado;
- operação solicitada: `kubectl get pods --request-timeout=10s`, estritamente read-only;
- credencial-base: deliberadamente não copiada;
- credencial, cache, grants e auditoria: ausentes antes da chamada;
- observação de preparo: `target add` deixou rules específico ausente; R0 explícito foi materializado conforme o gate antes da invocação;
- resultado: `PASS`.

Todos os arquivos de rules, SHA-256 `a9fcb303e907187487420f83932db545f81c40184a85ee179867facc660d8758`:

```yaml
version: "1.0"
deny: []
accept: []
```

Ação humana prevista: clicar **Negar** na janela de autorização.

Esperado: `human-deny`/`denied-interface`; nenhuma autenticação, preflight, credencial, cache, grant ou `ran`.

Resultados observados:

- operador clicou **Negar** e o processo foi monitorado até encerrar;
- resumo seguro: `deny` por `human-deny`, execução nula;
- eventos: `invoke`, `denied-interface`;
- preflight e `ran`: zero;
- credencial, cache e grants: ausentes;
- processos `torii` e `mcp_probe` remanescentes: zero;
- fim: `2026-07-15T19:28:05-03:00`;
- cleanup: `PASS`; raiz isolada removida e credencial-base preservada.

## LIVE-2026-07-15-K02 — K8S-AUTH-01

- caso: identidade válida no provider sem acesso ao target Kubernetes isolado;
- início do preparo: `2026-07-15T19:18:08-03:00`;
- raiz isolada: `C:\Users\paulo\AppData\Local\Temp\torii-live-K8S-AUTH-01-20260715-002`;
- target público: `lab-noaccess`; seleção local exigiu exatamente um context marcado como homologação; nome real não registrado;
- lifecycle provider: `aws`;
- operações: validator `sts get-caller-identity` e `kubectl get pods --request-timeout=10s`, estritamente read-only;
- rules: AWS e kubectl compartilhado em R0; target aceita somente `get pods`, com os mesmos hashes aprovados em K01;
- credencial-base de desenvolvimento: copiada byte a byte, sem `.session-cache`;
- grants e auditoria: ausentes antes da chamada;
- resultado: `PASS`.

Ação humana prevista: nenhuma.

Esperado: nenhuma GUI; lifecycle AWS e preflight passam; um `ran` do kubectl retorna `unauthorized` ou `forbidden`; rede/timeout/exit zero não contam como PASS; nenhuma saída completa é emitida.

Resultados observados:

- processo monitorado até o encerramento; nenhuma GUI;
- resumo seguro: decisão `allow` por `rules`, exit code `1`, stdout ausente, stderr `974` bytes classificado como `unauthorized`, sem truncamento;
- conteúdo completo do stderr não foi emitido nem retido;
- eventos, em ordem: `invoke`, `allowed-by-rules`, `preflight-provider`, `session-ok`, `preflight-ok`, `ran`;
- lifecycle e preflight: sucesso; `ran`: um com exit code `1`;
- credencial e cache existiram somente dentro da raiz; grants ausentes;
- processos `torii` e `mcp_probe` remanescentes: zero;
- fim: `2026-07-15T19:18:52-03:00`;
- cleanup: `PASS`; raiz isolada removida e credencial-base correta preservada.

## LIVE-2026-07-15-K11 — K8S-TGT-04

- caso: bloqueio de `--kubeconfig` pelo target;
- início do preparo: `2026-07-15T19:51:52-03:00`;
- raiz isolada: `C:\Users\paulo\AppData\Local\Temp\torii-live-K8S-TGT-04-20260715-011`;
- target público: `lab`; context real não registrado;
- argumento sintético: `get pods --kubeconfig arquivo-ficticio`; operação base estritamente read-only;
- AWS, kubectl compartilhado e target em R0 explícito, SHA-256 `a9fcb303e907187487420f83932db545f81c40184a85ee179867facc660d8758`;
- credencial-base: deliberadamente não copiada;
- resultado: `PASS`.

Rules aprovadas em todos os três escopos:

```yaml
version: "1.0"
deny: []
accept: []
```

Ação humana prevista: nenhuma.

Esperado: opção locked rejeitada antes de qualquer leitura; nenhuma GUI, autenticação, leitura de credencial, preflight, auditoria ou execução.

Resultados observados:

- resumo seguro: `is_error: true`, classe `blocked-option`, decisão e execução nulas;
- credenciais, cache e grants: zero antes e depois;
- `torii.log`: ausente, confirmando zero eventos `invoke` e `ran`;
- processos `torii` e `mcp_probe` remanescentes: zero;
- fim: `2026-07-15T19:52:02-03:00`;
- cleanup: `PASS`; raiz isolada removida com caminho validado; credencial-base preservada.

## LIVE-2026-07-15-K12 — K8S-TGT-05

- caso: bloqueio de `--server` pelo target;
- início do preparo: `2026-07-15T19:53:29-03:00`;
- raiz isolada: `C:\Users\paulo\AppData\Local\Temp\torii-live-K8S-TGT-05-20260715-012`;
- target público: `lab`; context real não registrado;
- argumento sintético: `get pods --server=https://invalid.example`; operação base estritamente read-only;
- AWS, kubectl compartilhado e target em R0 explícito, SHA-256 `a9fcb303e907187487420f83932db545f81c40184a85ee179867facc660d8758`;
- credencial-base: deliberadamente não copiada;
- resultado: `PASS`.

Rules aprovadas em todos os três escopos:

```yaml
version: "1.0"
deny: []
accept: []
```

Ação humana prevista: nenhuma.

Esperado: opção locked rejeitada antes de rede; nenhuma GUI, autenticação, leitura de credencial, preflight, auditoria ou execução.

Resultados observados:

- resumo seguro: `is_error: true`, classe `blocked-option`, decisão e execução nulas;
- credenciais, cache e grants: zero antes e depois;
- `torii.log`: ausente, confirmando zero eventos `invoke` e `ran`;
- processos `torii` e `mcp_probe` remanescentes: zero;
- fim: `2026-07-15T19:53:39-03:00`;
- cleanup: `PASS`; raiz isolada removida com caminho validado; credencial-base preservada.

## LIVE-2026-07-15-K13 — K8S-TGT-06

- caso: bloqueio de `--token` pelo target;
- início do preparo: `2026-07-15T19:54:59-03:00`;
- raiz isolada: `C:\Users\paulo\AppData\Local\Temp\torii-live-K8S-TGT-06-20260715-013`;
- target público: `lab`; context real não registrado;
- argumento declaradamente sintético: `get pods --token=valor-ficticio`; operação base estritamente read-only;
- AWS, kubectl compartilhado e target em R0 explícito, SHA-256 `a9fcb303e907187487420f83932db545f81c40184a85ee179867facc660d8758`;
- credencial-base: deliberadamente não copiada;
- resultado: `PASS`.

Rules aprovadas em todos os três escopos:

```yaml
version: "1.0"
deny: []
accept: []
```

Ação humana prevista: nenhuma.

Esperado: opção locked rejeitada antes das camadas sensíveis; nenhuma GUI, autenticação, leitura de credencial, preflight, auditoria ou execução.

Resultados observados:

- resumo seguro: `is_error: true`, classe `blocked-option`, decisão e execução nulas;
- credenciais, cache e grants: zero antes e depois;
- `torii.log`: ausente, confirmando zero eventos `invoke` e `ran`;
- processos `torii` e `mcp_probe` remanescentes: zero;
- fim: `2026-07-15T19:55:10-03:00`;
- cleanup: `PASS`; raiz isolada removida com caminho validado; credencial-base preservada.

## LIVE-2026-07-15-K14 — K8S-TGT-07

- caso: rules específico do target substitui o compartilhado;
- início do preparo: `2026-07-15T19:57:35-03:00`;
- raiz isolada: `C:\Users\paulo\AppData\Local\Temp\torii-live-K8S-TGT-07-20260715-014`;
- target público: `lab`; context real não registrado;
- operação solicitada: `kubectl get pods`, estritamente read-only;
- credencial-base: deliberadamente não copiada;
- resultado: `PASS`.

AWS em R0, SHA-256 `a9fcb303e907187487420f83932db545f81c40184a85ee179867facc660d8758`:

```yaml
version: "1.0"
deny: []
accept: []
```

kubectl compartilhado, SHA-256 `e938abc1cbb0cce0a2cc7a10f20c2592868fced52402b437e7a35cd0aa6c4f49`:

```yaml
version: "1.0"
deny: []
accept:
  - "get pods"
```

Target `lab`, SHA-256 `3580e7e0acb61a8ab5a9f7264542f36a65ffb243eae481df1d3726d77607d3a9`:

```yaml
version: "1.0"
deny:
  - "get pods"
accept: []
```

Ação humana prevista: nenhuma.

Esperado: `explicit-deny` do target apesar do accept compartilhado; nenhuma GUI, autenticação, leitura de credencial, preflight ou execução.

Resultados observados:

- decisão `deny` por `explicit-deny`, execução nula;
- eventos, em ordem: `invoke`, `denied-explicit`;
- preflight e `ran`: zero;
- credenciais, cache e grants: zero antes e depois;
- processos `torii` e `mcp_probe` remanescentes: zero;
- fim: `2026-07-15T19:57:51-03:00`;
- cleanup: `PASS`; raiz isolada removida com caminho validado; credencial-base preservada.

## LIVE-2026-07-15-K15 — K8S-TGT-08

- caso: isolamento de grant entre targets;
- início do preparo: `2026-07-15T20:00:15-03:00`;
- raiz isolada: `C:\Users\paulo\AppData\Local\Temp\torii-live-K8S-TGT-08-20260715-015`;
- targets públicos: `lab` e `lab-noaccess`; contexts reais não registrados;
- operação em ambos: `kubectl get pods --request-timeout=10s`, estritamente read-only;
- AWS, kubectl compartilhado e os dois targets em R0 explícito, SHA-256 `a9fcb303e907187487420f83932db545f81c40184a85ee179867facc660d8758`;
- credencial-base: copiada byte a byte, sem copiar `.session-cache`;
- grants e auditoria: ausentes antes da primeira chamada;
- resultado: `PASS`.

Rules aprovadas nos quatro escopos:

```yaml
version: "1.0"
deny: []
accept: []
```

Ações humanas previstas: permitir a primeira chamada por `2` minutos; negar a segunda chamada no outro alias.

Esperado: primeira chamada `human-grant` e `ran`; segunda chamada abre autorização própria e termina em `human-deny`, sem segundo `ran`; grant existe somente no alias de origem.

Resultados observados:

- primeira invocação no `lab`: `allow` por `human-grant`, exit zero, stdout `213` bytes não exibido e stderr ausente;
- após a primeira chamada: um grant no `lab` e zero no `lab-noaccess`;
- segunda invocação no `lab-noaccess`: janela reapareceu e o operador negou; decisão `deny` por `human-deny`, execução nula;
- eventos, em ordem: `invoke`, `override-timed`, `preflight-provider`, `session-ok`, `preflight-ok`, `ran`, `invoke`, `denied-interface`;
- totais: duas invocações, um grant temporário, uma negação de interface e exatamente um `ran`;
- credencial e cache existiram somente dentro da raiz durante o caso;
- processos `torii` e `mcp_probe` remanescentes: zero;
- fim: `2026-07-15T20:02:05-03:00`;
- cleanup: `PASS`; raiz, cópia da credencial, cache e grant removidos com caminho validado; credencial-base preservada.

## Fechamento da sessão

- encerramento: `2026-07-15T20:05:13-03:00`;
- resumo histórico: `17` instâncias `PASS` e `2` instâncias `FAIL`; as duas falhas preservam as tentativas anteriores à correção da credencial-base;
- todos os casos catalogados da rodada Kubernetes passaram após a correção da credencial;
- credencial-base removida com caminho validado;
- raízes temporárias `torii-live-*` remanescentes: zero;
- processos `torii` e `mcp_probe` remanescentes: zero;
- busca por nomes de context, ARN e identificadores de conta em `docs/` e `tests/`: zero ocorrência;
- `cargo fmt --all -- --check`: `PASS`;
- `cargo check --all-targets`: `PASS`;
- `cargo test --all-targets`: `PASS`, `46` testes;
- `cargo clippy --all-targets -- -D warnings`: `PASS`;
- `mdbook build docs`: `PASS`.
