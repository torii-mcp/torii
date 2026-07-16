# Modelo de segurança

O Torii reduz a superfície de execução disponível ao agente. Ele não transforma um CLI ou credencial de alto privilégio em algo seguro por si só; política local e privilégios remotos precisam ser configurados em conjunto.

## Invariantes

1. **Default deny.** Ausência de regra ou grant não significa permissão.
2. **Deny prioritário.** Um deny compatível encerra a avaliação antes de qualquer aprovação.
3. **Autorização antes de autenticação.** `.env`, credenciais e cache não são lidos no caminho negado.
4. **Sem shell.** O programa recebe argumentos via `Command::args`.
5. **Credencial por processo.** Material coletado pelo Torii é sobreposto somente nos filhos de validação e execução.
6. **Substituição após validação.** Reauth falho preserva a sessão anterior.
7. **Concorrência serializada por provider de lifecycle.** Chamadas simultâneas que herdam o mesmo provider compartilham seu lock.
8. **Auditoria sanitizada.** Logs usam uma referência curta e não armazenam clipboard, credenciais ou saída completa.
9. **Target sob controle humano.** O alias resolve para configuração local; flags de troca de context, identidade e endpoint são bloqueadas.
10. **Política pertence ao operador.** Install cria rules vazio; setup é o único writer e update nunca toca em rules ou estado operacional.
11. **Lifecycle herdado pelo target.** Todo target indica um provider instalado. Somente depois de allow o Torii lê o ambiente e executa o lifecycle desse provider; o ambiente resultante é aplicado apenas ao processo filho alvo.
12. **Grant tokenizado.** A invocação exata compara todos os tokens e seu tamanho; um grant de prefixo compara somente o prefixo explicitamente escolhido pelo operador. Nenhum grant é reconstruído como linha de shell.

## Ordem crítica

```text
validar envelope MCP
        |
resolver provider/target e bloquear overrides
        |
carregar rules.yaml
        |
deny / accept / grant / aprovação
        |
somente se permitido: carregar env e sessão
        |
lifecycle do provider indicado pelo target
        |
executar provider
```

Um refactor que antecipe a leitura de credenciais para antes da decisão é uma regressão de segurança, mesmo que o comando continue sendo bloqueado depois.

## Grants temporários

O operador escolhe entre uma invocação exata e um prefixo de argumentos. Prefixo é uma ampliação explícita: qualquer sufixo posterior pode variar, desaparecer ou ser acrescentado. A interface apresenta o vetor como tokens, não como uma linha de shell, e a confirmação é reiniciada quando duração ou escopo mudam.

O arquivo persistido contém somente o tipo, o tamanho e o fingerprint tokenizado do matcher. Arquivos legados ou corrompidos não autorizam chamadas.

## Ambiente herdado

O processo filho herda o ambiente do processo Torii e recebe por cima `.env` e a sessão do provider. O Torii não chama `env_clear`. Portanto:

- não inicie o servidor com segredos globais desnecessários;
- use variáveis como `AZURE_CONFIG_DIR` e `CLOUDSDK_CONFIG` no `.env` do provider quando o CLI suportar isolamento;
- execute o Torii sob a mesma conta local confiável que controla seus arquivos de configuração.

## Limites

A versão atual não oferece sandbox de sistema operacional, timeout de processo, streaming de saída, daemon multiusuário, assinatura de providers ou distribuição remota. O runner limita o conteúdo devolvido ao agente, mas o processo pode produzir mais dados internamente antes da captura terminar.
