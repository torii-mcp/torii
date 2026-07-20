# Fluxo de uma chamada

Considere a chamada à tool `aws`:

```json
{ "args": ["ec2", "describe-instances", "--region", "sa-east-1"] }
```

## 1. Dispatch MCP

O servidor confirma que a tool existe, rejeita campos extras e exige pelo menos um item em `args`.

Em provider target-aware, exige um alias conhecido, recusa flags bloqueadas e resolve o binding, paths e lock daquele target antes de qualquer leitura de ambiente. `kubectl_context` resolve um context; `aws_profile` resolve profile, conta esperada e região opcional sem expor esses valores ao MCP. O alias pode aparecer no schema e ainda estar inativo: configuração e lease são estados diferentes.

## 2. Jasper

O provider é localizado e `rules.yaml` é carregado. Jasper verifica todos os denies antes dos accepts. O resultado é:

- `DeniedExplicit`: encerra sem credenciais ou processo;
- `Allowed`: registra a regra compatível;
- `Unresolved`: procura grant ativo e, se necessário, pede decisão humana.

Em uma tool target-aware, o `DeniedExplicit` retorna antes de qualquer tela de lease. Para os demais caminhos, o dispatcher lê o lease do provider e confere expiração e digest do binding antes de procurar grant ou abrir a aprovação da operação. Se o alias estiver inativo, a janela privada mostra o binding humano solicitado e reúne os aliases já ativos em uma única seção compacta. A duração fica no mesmo card do target solicitado, aceita 1 a 1.440 minutos e começa em `default_target_minutes`. O humano escolhe **Substituir** todos os ativos, **Adicionar** ao conjunto ou **Negar**. Quando **Adicionar** criar múltiplos ativos, um alerta em largura completa aparece imediatamente acima das ações e a confirmação só conclui depois de manter o botão pressionado por 2 segundos, com o progresso visível no próprio botão.

Um lease de target não é grant Jasper: ele libera apenas a passagem para a política e para a autenticação daquele alias. Se mais de um alias ficar ativo, o agente pode escolher qualquer um deles nas operações que a política permitir.

A janela de uma decisão `Unresolved` mostra os argumentos como tokens e cresce por estados: compacta para uma execução, intermediária para grant `exact` e expandida para edição de prefixo. A largura não muda; o redimensionamento preserva o centro atual da janela. Argumentos longos aparecem com começo, fim e tamanho, e seu conteúdo original pode ser revisado em páginas sem alterar o vetor usado no matcher ou na execução.

Ao escolher uma permissão temporária, o Torii sugere uma fronteira antes do primeiro argumento iniciado por `-`, desde que existam pelo menos dois tokens anteriores. A sugestão é apenas estrutural, vem acompanhada do motivo e pode ser restaurada depois de uma edição; se a fronteira for precoce ou não existir, a invocação exata permanece selecionada. O operador ainda escolhe livremente `exact` ou qualquer prefixo válido.

No editor, os tokens fixos e variáveis ficam em grupos rotulados e um marcador explícito lembra que qualquer prefixo aceita também argumentos futuros. Um resumo destacado acompanha o estado logo acima das ações. **Negar** mostra brevemente o resultado em coral; **Permitir** mostra em verde a autorização única ou sua duração antes de prosseguir. O botão de permissão permanece desabilitado até o operador confirmar que revisou invocação, target e escopo.

Esse feedback visual não muda a ordem de segurança: deny explícito nunca abre a janela, e autenticação só começa depois que uma decisão não explícita foi permitida.

## 3. Ambiente e sessão

Depois da decisão da operação, o Torii relê o lease. Somente se ele ainda for válido lê o `.env` compartilhado e, quando houver, o `.env` do target. Em seguida, o lifecycle garante uma sessão válida no escopo. Para `aws_profile`, o Torii aplica o profile fixado, remove overrides AWS herdados e consulta STS; conta divergente encerra antes do comando pedido.

## 4. Runner

O processo é construído conceitualmente assim:

```rust,ignore
Command::new(provider.command)
    .args(provider.args_prefix)
    .args(target.args_prefix)
    .args(request.args)
    .envs(persistent_env)
    .envs(auth_env);
```

Antes do launch, o lease é conferido uma última vez. Stdin do filho é nulo. Stdout e stderr são capturados. Um processo encerrado sem exit code recebe o fallback `143`. Um `target clear` posterior não mata esse processo, mas impede launches que ainda não passaram por essa conferência.

## 5. Resultado

Torii devolve provider, target quando aplicável, decisão e, quando houve execução, exit code, stdout, stderr e indicador de truncamento. Exit code diferente de zero pertence ao provider e continua visível no resultado.

## Concorrência

Chamadas que herdam providers diferentes podem validar sessões independentemente. Chamadas concorrentes que usam o mesmo provider de lifecycle compartilham o lock desse provider. O estado de lease usa também um arquivo de lock exclusivo do sistema operacional e revisão persistida, pois `target activate`, `target clear` e servidores MCP podem coexistir; o handle é liberado automaticamente no término do processo, sem TTL ou limpeza por timeout de lock stale.

Aliases `aws_profile` compartilham lock somente consigo mesmos, pois cada target possui seu próprio escopo de autenticação.
