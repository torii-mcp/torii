# Modelo de segurança

O Torii reduz a superfície de execução disponível ao agente. Ele não transforma um CLI ou credencial de alto privilégio em algo seguro por si só; política local e privilégios remotos precisam ser configurados em conjunto.

## Invariantes

1. **Default deny.** Ausência de regra ou grant não significa permissão.
2. **Deny prioritário.** Um deny compatível encerra a avaliação antes de qualquer aprovação.
3. **Autorização antes de autenticação.** `.env`, credenciais e cache não são lidos no caminho negado.
4. **Sem shell.** O programa recebe argumentos via `Command::args`.
5. **Credencial por processo.** Material coletado pelo Torii é sobreposto somente nos filhos de validação e execução.
6. **Substituição após validação.** Reauth falho preserva a sessão anterior.
7. **Concorrência serializada por escopo de autenticação.** Chamadas que herdam o mesmo provider compartilham seu lock; cada alias `aws_profile` possui um lock próprio.
8. **Auditoria sanitizada.** Logs usam uma referência curta e não armazenam clipboard, credenciais ou saída completa.
9. **Target sob controle humano.** O alias resolve para configuração local; flags de troca de context, identidade e endpoint são bloqueadas.
10. **Política pertence ao operador.** Install cria rules vazio; setup é o único writer e update nunca toca em rules ou estado operacional.
11. **Lifecycle herdado pelo target.** Todo target indica um provider instalado. Somente depois de allow o Torii lê o ambiente e executa o lifecycle desse provider; o ambiente resultante é aplicado apenas ao processo filho alvo.
12. **Grant tokenizado.** A invocação exata compara todos os tokens e seu tamanho; um grant de prefixo compara somente o prefixo explicitamente escolhido pelo operador. Nenhum grant é reconstruído como linha de shell.
13. **Consulta de política somente leitura.** `torii_policy` pode ler as regras ativas de um provider ou target, mas não lê ambiente, credenciais, cache ou grants e não altera estado.
14. **Binding AWS humano.** Um target `aws_profile` fixa profile, região opcional e conta esperada fora do MCP; o Torii remove overrides herdados, bloqueia overrides do agente e confirma a conta por STS antes de cada execução permitida.
15. **Lease humano de target.** Um alias target-aware é configurado, mas começa inativo. Depois de avaliar o deny explícito — que encerra a chamada se compatível — e antes de grants, ambiente, sessão ou execução, o dispatcher exige um lease humano ainda válido para aquele binding.

## Ordem crítica

```text
validar envelope MCP
        |
resolver provider/target e bloquear overrides
        |
carregar rules.yaml
        |
deny explícito
        |
lease humano do target, quando aplicável
        |
accept / grant / aprovação da operação
        |
revalidar lease; somente se permitido: carregar env e sessão
        |
lifecycle do provider de identidade, no balde do escopo
        |
quando há identity.expect: conferir identidade pelo probe do provider
        |
revalidar lease
        |
executar provider
```

Um refactor que antecipe a leitura de credenciais para antes da decisão é uma regressão de segurança, mesmo que o comando continue sendo bloqueado depois.

## Grants temporários

O operador escolhe entre uma invocação exata e um prefixo de argumentos. Prefixo é uma ampliação explícita: qualquer sufixo posterior pode variar, desaparecer ou ser acrescentado. A interface apresenta o vetor como tokens, não como uma linha de shell, e a confirmação é reiniciada quando duração ou escopo mudam.

O arquivo persistido contém somente o tipo, o tamanho e o fingerprint tokenizado do matcher. Arquivos legados ou corrompidos não autorizam chamadas.

## Leases de target

Criar um alias não o ativa. O lease autoriza temporariamente o uso daquele binding humano — por exemplo, um context Kubernetes ou um profile AWS — mas não autoriza nenhuma operação por si só. A política Jasper e seus grants continuam sendo avaliados depois.

Todos os aliases configurados continuam visíveis no schema MCP, inclusive inativos. Isso permite que uma chamada a um alias conhecido peça decisão humana; esconder aliases inativos no schema impediria essa fronteira. O estado fica por provider, contém expiração e um digest do binding. Alterar, remover ou recriar o binding invalida o lease anterior.

A interface humana mostra o binding solicitado e os aliases ativos. **Substituir** desativa todos os demais e ativa o solicitado. **Adicionar** preserva os ativos. Quando isso resultar em mais de um, um alerta em largura completa fica imediatamente acima das ações, avisa que o agente poderá escolher qualquer alias ativo em operações permitidas e exige manter **Adicionar** pressionado por 2 segundos; soltar antes interrompe a confirmação. **Negar** não altera o estado. A duração vai de 1 a 1.440 minutos; o padrão é `default_target_minutes`, inicialmente 15.

O estado possui revisão, CAS (comparação-e-troca) antes da escrita, um arquivo de lock exclusivo do sistema operacional entre processos e persistência atômica. O handle do lock é liberado automaticamente ao término ou falha do processo; não há TTL ou limpeza por timeout de um lock considerado stale. Assim, uma escolha feita numa janela antiga não restaura um lease depois de `target clear` ou outra alteração. O lease é conferido novamente antes de ambiente/autenticação e imediatamente antes do launch; uma revogação ou expiração bloqueia uma chamada ainda pendente, mas não encerra um processo já iniciado.

## Ambiente herdado

O processo filho herda o ambiente do processo Torii e recebe por cima `.env` e a sessão do provider. O Torii não chama `env_clear`. Portanto:

- não inicie o servidor com segredos globais desnecessários;
- use variáveis como `AZURE_CONFIG_DIR` e `CLOUDSDK_CONFIG` no `.env` do provider quando o CLI suportar isolamento;
- execute o Torii sob a mesma conta local confiável que controla seus arquivos de configuração.

`aws_profile` é a exceção controlada: para o filho AWS daquele alias, o Torii remove variáveis herdadas de credencial, região e endpoint que poderiam sobrepor o profile fixado. O restante do ambiente continua preservado.

## Limites

A versão atual não oferece sandbox de sistema operacional, timeout de processo, streaming de saída, daemon multiusuário, assinatura de providers ou distribuição remota. O runner limita o conteúdo devolvido ao agente, mas o processo pode produzir mais dados internamente antes da captura terminar.
