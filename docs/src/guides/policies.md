# Escrever políticas

Para abrir a política no seu editor, com validação antes de aplicar:

```powershell
torii policy show aws                    # imprime a política ativa e seu caminho
torii policy edit aws                    # edita a política compartilhada do provider
torii policy edit kubectl dev            # edita a política daquele alias
torii policy edit kubectl dev --create   # cria a política do alias
```

`policy edit` trabalha sobre uma cópia: ao fechar o editor, o Torii parseia o YAML e compila cada regra, e só então substitui o arquivo vivo. Uma regex inválida ou um YAML malformado é recusado antes de virar política, e o rascunho é preservado para você corrigir. Nada impede editar `rules.yaml` à mão — o comando existe para que um erro de digitação não vire uma negação inesperada no meio de uma sessão do agente. Detalhes em [CLI de controle](../reference/cli.md).

Cada provider possui seu próprio `rules.yaml`. Em provider target-aware, um `rules.yaml` dentro do target compõe com o compartilhado: os denies somam — um deny da raiz vale em todo alias e não pode ser removido ali — e os accepts do target substituem os compartilhados naquele alias. Detalhes em [camadas](../concepts/jasper.md#camadas-em-provider-target-aware).

Antes de Jasper consultar `accept`, grants ou uma aprovação de operação para um target-aware, o alias precisa de lease humano válido. O lease autoriza o uso temporário do binding, não a operação; rules e grants continuam sendo necessários. Revogar o lease não apaga grants existentes, mas impede seu uso até que o alias seja ativado novamente.

```yaml
version: "1.0"
deny:
  - "secretsmanager get-secret-value"
  - "ecs execute-command"
accept:
  - "s3 ls"
  - "ec2 describe-instances"
```

## Decisão permanente pela janela

Na janela de autorização de uma chamada não resolvida, escolher **Temporariamente** e depois um **prefixo** abre o editor de escopo. No canto superior direito da caixa `PERMANECEM IGUAIS` — a que mostra os argumentos fixos — ficam dois botões de **manter pressionado**: **Sempre permitir** grava o prefixo no `accept` e executa a chamada; **Sempre negar** grava no `deny` e a nega.

A ordem é deliberada: primeiro o ajuste fino da fronteira, depois a decisão permanente. A regra gravada é exatamente o prefixo que está na caixa, não o argv inteiro — mover a fronteira muda a regra, e mover a fronteira também descarta qualquer gesto em curso.

O gesto leva **cinco segundos** de botão pressionado, e os segundos são o ponto: eles existem para você ter tempo de perguntar se aquilo deve mesmo entrar na política para sempre. Um clique rápido não faz nada, soltar no meio descarta o progresso, e os botões ficam inertes no primeiro segundo e meio de janela aberta — um prompt é aberto por uma chamada do agente e pode nascer sob um clique que já estava em curso.

Nada permanente é oferecido em **Uma vez** nem em **Somente esta invocação exata**: uma regra literal sempre casa por prefixo, então não existe regra que expresse "só esta invocação, nada acrescentado". A janela diz isso no lugar dos botões.

A regra de cada fronteira é construída do **argv normalizado daquele prefixo** — o que a avaliação realmente vê, não o que a tela mostra. Cada fronteira é normalizada por conta própria, porque normalizar o argv inteiro e cortar depois erraria: uma flag exatamente na fronteira não tem valor seguinte para descartar.

Um botão só fica disponível na fronteira em que a regra realmente funcionaria. Antes de abrir a janela, o Torii simula a política resultante sobre o argv daquele prefixo; se o veredicto não for o pretendido, o botão nasce desabilitado com o motivo no tooltip, em vez de falhar depois do gesto. Os casos recusados:

| Situação | Por quê |
|---|---|
| Um argumento do prefixo contém espaço | Regras literais são tokenizadas por whitespace: a regra teria mais tokens que o argv e nunca casaria |
| A normalização esvazia aquele prefixo | Não sobra regra para escrever naquela fronteira |
| Accept abaixo de `minimum_accept_tokens` | Seria ignorado na avaliação; o deny da mesma fronteira continua disponível, porque deny não tem largura mínima |
| Já existe deny explícito compatível | Deny vence accept, então o accept não mudaria nada |
| Target sem `rules.yaml` próprio | A regra iria para a política compartilhada e valeria em todos os targets; crie a política do alias com `--create` |
| O `rules.yaml` não está na forma simples (vetor em linha, chave repetida) | A inserção preserva comentários por reescrita textual e recusa formas que não entende, em vez de desfigurar o arquivo |

A escrita é feita pelo processo servidor, nunca pela janela. A janela devolve só a fronteira confirmada; o servidor reconstrói a regra a partir dela, relê a política do disco no momento da gravação — para não sobrescrever um `torii policy edit` concorrente — e parseia, compila e simula o texto resultante antes de substituir o arquivo de forma atômica. Comentários, indentação e fim de linha do arquivo são preservados. Cada escrita entra na auditoria como `policy-accept-added` ou `policy-deny-added`.

Se a gravação falhar, a chamada segue apenas o gesto — permitida uma vez ou negada — e a falha aparece na auditoria como `policy-write-failed` e no stderr do servidor. Nada é silenciosamente tratado como se a política tivesse mudado.

Não há desfazer na janela. Para reverter, use `torii policy edit` e apague a linha.

## Comece pelo mínimo

Adicione somente operações observadas e necessárias. A ausência de uma operação não impede aprovação humana quando a GUI está habilitada, mas em headless ela será negada.

## Use deny para escapes conhecidos

Bloqueie comandos que abrem execução arbitrária, túneis, proxies ou leitura direta de segredos. Deny vence mesmo se uma regra accept mais ampla também casar.

## Regras por regex

Uma regra delimitada por `/…/flags` é tratada como regex e casa em **qualquer posição** do argv (não só no prefixo). Serve para inspecionar conteúdo — por exemplo, negar palavras destrutivas dentro de uma query SQL inline:

```yaml
deny:
  - "/\\btruncate\\b/i"
  - "/copy\\s+into/i"
```

O padrão é tudo entre a primeira e a última barra; o trecho final são as flags (`i` case-insensitive, `m` multi-line, `s` dot-matches-newline, `x` ignore-whitespace). Regras regex não estão sujeitas a `minimum_accept_tokens`. Um regex inválido faz a avaliação **falhar fechada** (erro, nunca allow silencioso), então cubra os exemplos com um teste.

Regex é best-effort: concatenação dinâmica e stored procedures escapam. Trate-o como defense-in-depth e auditoria — a fronteira dura deve estar no próprio serviço (ex.: um role read-only). Veja os providers `snow` e `az` em `examples/providers/` para políticas completas.

## Posicione flags depois da ação

Jasper avalia prefixos desde o primeiro item. Prefira:

```text
get pods -n equipe
```

Evite:

```text
-n equipe get pods
```

O segundo formato não casa com `get pods` e será não resolvido.

## Escolha grants conscientemente

Ao permitir temporariamente, escolha entre a invocação `exact` e um prefixo de argumentos. A interface pode sugerir um prefixo imediatamente antes do primeiro argumento iniciado por `-`, quando há pelo menos dois tokens anteriores. Essa é uma sugestão pelo formato do vetor, não a dedução de uma operação semântica; revise e mova a fronteira quando necessário.

Um prefixo de `get pods`, por exemplo, permite chamadas futuras que comecem exatamente por esses dois tokens no mesmo target; argumentos posteriores podem mudar, desaparecer ou ser acrescentados. Mesmo quando todos os argumentos atuais estão fixos, um prefixo ainda permite acrescentar novos argumentos no futuro.

Use o prefixo somente quando o conjunto autorizado estiver claro para o operador. A invocação exata exige o mesmo número, valores e ordem de argumentos. Não passe segredos em argumentos de CLI.

## Teste fronteiras

Ao alterar matching, cubra pelo menos:

- deny e accept para a mesma ação;
- prefixos parecidos como `s3` e `s3api`;
- accept abaixo do mínimo;
- comando não listado;
- grant expirado e ativo.
