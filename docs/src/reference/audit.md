# Auditoria

O Torii registra **toda chamada de agente** e todo comando humano que muda estado, para que o humano possa acompanhar ao vivo, buscar, inspecionar e exportar o que aconteceu. Como muita coisa passa por regra ou grant sem janela, é aqui que o humano descobre o que foi executado em seu nome.

## O que é registrado

Uma chamada MCP vira um **registro** com número sequencial, aberto antes de qualquer decisão. Por isso, até uma chamada recusada por um target desconhecido ou por uma flag travada deixa rastro. O registro guarda:

| Campo | Conteúdo |
|---|---|
| início e fim | horário local, com milissegundos nos passos |
| provider e target | a tool que o agente chamou e o alias pedido |
| chamada | o `args` exatamente como o agente enviou |
| motivo | a `description` do agente, como texto não verificado |
| veredicto | `ALLOW`, `DENY`, `ERROR` ou `RUN` (ainda rodando ou interrompido) |
| como | regra `accept`, grant temporário, aprovação humana (uma vez, por tempo ou permanente), regra `deny`, negação na janela, negação em headless, argumento proibido, target sem lease, lease perdido |
| regra | o `accept`, o `deny`, o grant ou a regra permanente gravada por trás do veredicto |
| resultado | exit code, tamanho de stdout e stderr e se a saída foi truncada |
| erro | a mensagem, quando a chamada falhou |
| saída | stdout e stderr devolvidos ao agente, compactados (com `log-output full`, o padrão) |

Cada passo da chamada vira um **evento** no registro, com horário e escopo: a sessão conferida ou renovada no provider de identidade, a identidade confirmada, o lease pedido ou perdido, o grant que casou, a regra gravada pela janela.

Comandos humanos que mudam estado também viram registros, marcados `HUMAN`: ativação, limpeza, renomeação e remoção de lease de target, `reauth`, e criação ou edição de política (com as regras que entraram e saíram de cada vetor).

**Nunca são registrados:** credenciais, valores do `.env`, conteúdo do clipboard e identidades observadas pelo probe. Estas só aparecem no erro devolvido à chamada; quando esse erro é registrado, ele fica cifrado como o resto.

## Onde fica e como é protegido

```text
<raiz>/
├── audit.db      SQLite, o arquivo inteiro cifrado
├── audit.lock    serializa a criação da chave entre processos
└── audit.key     só sem cofre do sistema (veja abaixo)
```

O banco é um SQLite3 Multiple Ciphers: o arquivo inteiro, inclusive o WAL, é cifrado página a página com ChaCha20-Poly1305, e cada página é autenticada, então uma página alterada não abre. Do arquivo, sem a chave, só se sabe o tamanho. Quem copiar o disco não lê nem as datas, nem os providers, nem as decisões, nem a estrutura das tabelas. Dentro do banco aberto, os registros são colunas comuns com índices, e a busca é SQL.

O SQLite3 Multiple Ciphers entra no lugar do SQLite do `rusqlite` por uma cópia do `libsqlite3-sys` em `vendor/`, descrita em `vendor/README.md`. Ele não traz OpenSSL nem dependência de sistema: o binário continua único, sem DLL.

A chave é gerada uma vez por raiz de configuração e fica no cofre do sistema operacional:

| Sistema | Cofre |
|---|---|
| Windows | Gerenciador de Credenciais, protegido pelo DPAPI com o login do usuário |
| macOS | Keychain |
| Linux | Secret Service (GNOME Keyring, KWallet) |

Sem cofre, como num servidor Linux sem Secret Service, a chave vai para `audit.key`, legível só pelo dono (permissão `600`), e o Torii avisa no stderr. **Nesse caso, quem tiver o disco tem a chave.** `TORII_AUDIT_KEYSTORE=file` força esse modo de propósito. O `cargo` do repositório o define (em `.cargo/config.toml`) para que testes não gravem no cofre do desenvolvedor.

A chave protege contra cópia do disco, backup e outros usuários. Ela **não** protege contra um processo do mesmo usuário na mesma máquina: esse processo pode pedir a chave ao cofre como o próprio Torii faz.

Se o banco não abrir com a chave da máquina (a chave foi perdida ou o arquivo veio de outra máquina), ele é movido para `audit-<data>.unreadable.db`, nunca sobrescrito, e um banco novo começa.

## Acompanhar, buscar e exportar

```powershell
torii log tail                     # últimos registros, uma linha cada
torii log tail -n 50 --follow      # e continua acompanhando; Ctrl+C sai
torii log search --denied --since 2h
torii log search --provider aws --target prd --text "s3 rm"
torii log show 128                 # tudo sobre um registro
torii log show 128 --output        # e o que a chamada devolveu
torii log export --since 2026-10-01 --format md --file auditoria.md
torii log prune 30                 # mantém só os últimos 30 dias
```

Uma linha do `tail`:

```text
TIME                       ID  SCOPE        VERDICT  HOW              RESULT          CALL
2026-10-23 14:32:07      #128  aws/prd      ALLOW    temporary grant  exit 0   1.2s   s3api list-objects-v2 --bucket logs
2026-10-23 14:32:31      #129  kubectl/dev  DENY     deny rule                 3ms    delete pod api-7f9c
```

Com `--follow`, uma chamada aparece quando chega e de novo quando termina. Assim, uma chamada esperando a sua resposta numa janela aparece enquanto espera.

### Número do registro

O número é sequencial e nunca é reaproveitado, nem depois de um `prune`: um `#128` anotado num ticket continua apontando para a mesma chamada. Em `tail` e `search` ele fica alinhado à direita numa coluna de ao menos 7 dígitos, que só se alarga se o banco passar de 9.999.999 registros.

Num terminal, os **últimos dígitos** do número aparecem em negrito: são os que bastam para achar o registro. Como os números são sequenciais, a quantidade é a mesma em todas as linhas e depende só da faixa de números guardados. Num banco que nunca passou por `prune`, é o número inteiro. Depois que a retenção apaga os antigos, `#1015312` numa faixa de 15 mil registros mostra `15312` em negrito, e `torii log show 15312` basta. O número completo vale sempre; havendo mais de um registro com aqueles dígitos finais, o `show` lista os candidatos em vez de escolher. O negrito não aparece com `NO_COLOR`, com `TERM=dumb` nem quando a saída vai para um pipe ou arquivo.

Filtros de `search` e `export`:

| Filtro | Mantém |
|---|---|
| `--provider <tool>` / `--target <nome>` | a tool e o alias |
| `--allowed` `--denied` `--errors` `--running` | qualquer um dos veredictos marcados |
| `--agent` / `--human` | só chamadas de agente ou só comandos humanos |
| `--since <quando>` / `--until <quando>` | `30m`, `2h`, `7d`, `2026-10-20` ou `2026-10-20T14:30`, no horário local |
| `--text <palavras>` | na chamada, no motivo, na regra ou no erro, sem diferenciar maiúsculas |

`export` grava JSON (padrão) ou Markdown em `--file` ou no stdout; `--with-output` inclui a saída de cada chamada. **A exportação é texto aberto, não cifrado**: guarde-a num lugar seguro.

## Retenção e preferências

| Preferência | Padrão | Efeito |
|---|---|---|
| `torii config log-retention set <dias>` | 90 | o que passa disso sai no próximo prune |
| `torii config log-tail set <n>` | 20 | quantos registros `tail` e `search` mostram sem `-n` |
| `torii config log-output set full\|none` | `full` | guardar ou não stdout e stderr das chamadas |

Todo servidor MCP aplica a retenção quando inicia. `torii log prune [dias]` aplica na hora, com o número dado ou com a retenção, e apaga junto os passos e a saída de cada registro.

O volume esperado é pequeno: a saída já é cortada em `max_output_bytes` (256 KiB) e texto de CLI compacta de 5 a 10 vezes. Mil chamadas por dia com 20 KB de saída média dão uns 3 MB por dia.

## Comportamento em falha

A escrita é best-effort: uma falha do log avisa uma vez no stderr e a chamada continua como seguiria sem ele. Vários servidores Torii na mesma máquina gravam no mesmo banco ao mesmo tempo (SQLite em modo WAL).

O antigo `torii.log` em texto não é mais escrito. Um arquivo que já exista na raiz fica onde está; apague-o quando não precisar mais dele.

## Eventos

| Evento | Leitura |
|---|---|
| `allowed-by-rules` / `allowed-by-grant` | passou por accept ou por grant vivo |
| `denied-explicit` | casou um `deny`; nada mais foi consultado |
| `denied-forbidden-arg` | um argumento proibido pelo provider |
| `denied-interface` / `denied-headless` | negado na janela, ou sem janela porque `TORII_NO_GUI` está definido |
| `override-once` / `override-timed` | aprovado na janela, uma vez ou por tempo (com o grant criado) |
| `policy-accept-added` / `policy-deny-added` | regra permanente gravada pela janela, com a regra e a política |
| `policy-write-failed` | o gesto permanente não chegou ao disco; a chamada seguiu só o gesto |
| `invalid-accept` / `invalid-rule` | accept abaixo da largura mínima, ignorado; regra que não compila |
| `legacy-grants-ignored` / `invalid-grants-ignored` | arquivo de grants antigo ou corrompido; pede reaprovação |
| `target-access-requested` / `-denied` / `-replaced` / `-added` / `-stale` / `-lost` / `-revoked` / `-cleared` | o ciclo do lease de target |
| `preflight-provider` / `preflight-ok` / `preflight-failed` | autenticação pelo provider de identidade |
| `session-ok` / `session-unchecked` / `session-invalid` / `session-refreshed` / `reauth-forced` | estado da sessão do escopo de credencial |
| `session-candidate-invalid` | uma tentativa recusada na janela de autenticação, uma por tentativa, com o motivo dado pelo validador (por exemplo, `ExpiredToken` ou falha de certificado) |
| `identity-ok` / `identity-mismatch` | conferência da identidade esperada |
| `identity-check-failed` | o probe não disse de quem é a sessão, com o motivo: código de saída e última linha do stderr, saída truncada ou campo ausente |
| `ran` | o processo terminou, com o exit code |
| `policy-created` / `policy-edited` | comando humano: política criada ou editada, com as regras que mudaram |
