# Personalizar as janelas

As janelas de autorização e de autenticação seguem duas preferências do humano: o **tema** e a
**cor da barra dobrada**. As duas ficam no `settings.yaml` e são trocadas pelo `torii config`.
Cada janela lê as preferências quando abre, então a mudança vale a partir da próxima janela,
sem reiniciar o servidor MCP.

Toda ação é um verbo. Cada preferência responde aos mesmos quatro:

| Verbo | Efeito |
|---|---|
| `list` | lista as opções e marca o padrão e a que está em uso; `ls` também vale |
| `set <valor>` | grava o valor |
| `reset` | volta ao padrão |
| `test [<valor>]` | abre uma janela de autorização de exemplo com o valor, ou com o gravado, sem gravar nada |

```powershell
torii config list                  # cada preferência, o valor atual e o padrão
torii config theme test light      # vê o tema antes de escolher
torii config theme set light       # troca o tema
torii config fbc set salvia        # troca a cor da barra dobrada
torii config fbc reset             # volta ao padrão
```

## Janela de teste

`test` abre a janela de autorização de verdade, com um pedido de mentira: o agente fictício quer
pedir uma pizza família de abacaxi para a sala de reunião, e o motivo dele diz que é um teste de
tema do Torii. Todos os botões funcionam, inclusive os permanentes, mas nada é executado nem
gravado, qualquer que seja a resposta. Para ver a cor da barra dobrada, dobre a janela.

```powershell
torii config theme test            # no tema gravado
torii config theme test nord       # num tema, sem trocar o gravado
torii config fbc test '#c9a0dc'    # com uma cor na barra dobrada
```

## Tema

```powershell
torii config theme list
torii config theme set nord
torii config theme reset           # volta para dark
```

| Tema | Aparência |
|---|---|
| `dark` | escuro, o padrão |
| `light` | claro; os destaques ficam mais escuros para serem lidos sobre fundo claro |
| `nord` | a paleta [Nord](https://www.nordtheme.com), com superfícies azul-ardósia |

O tema vale para a janela inteira e ignora o modo claro ou escuro do sistema operacional.

## Cor da barra dobrada

Dobrada, a janela fica com a altura da barra de título e pode ser estacionada em qualquer lugar da
tela. Para ela não se perder sobre o que estiver atrás, a barra dobrada ganha uma cor própria.
O padrão é `rosa-antigo`. `folded-bar-color` tem o atalho `fbc`.

```powershell
torii config fbc list
torii config fbc set azul-aco      # uma cor da lista
torii config fbc set '#c9a0dc'     # qualquer cor em #rrggbb
torii config fbc set theme         # a mesma barra do tema, como na janela aberta
torii config fbc reset             # volta para rosa-antigo
```

| Nome | Cor |
|---|---|
| `rosa-antigo` | `#d9a7b4`, o padrão |
| `salvia` | `#a9c4a0` |
| `menta` | `#9fd3c1` |
| `azul-ceu` | `#9ec1d9` |
| `azul-aco` | `#8fa6bd` |
| `lavanda` | `#bab4e2` |
| `coral` | `#e9a899` |
| `pessego` | `#edbb95` |
| `manteiga` | `#e6d38c` |
| `ocre` | `#d3b46e` |
| `grafite` | `#26262a`, a barra escura das versões anteriores |

Os nomes ignoram maiúsculas e acentos: `Sálvia` e `salvia` são a mesma cor. No PowerShell,
coloque a cor em `#rrggbb` entre aspas, porque `#` começa um comentário.

O texto, os botões e a borda da barra dobrada saem da própria cor: texto escuro sobre cor clara e
texto claro sobre cor escura, com botões e borda em tons da mesma cor. Assim qualquer valor
continua legível. Aberta, a janela volta à barra do tema.

## No arquivo

`torii config` só grava valores válidos. Quem edita o `settings.yaml` à mão usa os mesmos nomes:

```yaml
theme: nord
folded_bar_color: '#c9a0dc'
```

Um valor desconhecido faz o servidor recusar o arquivo na partida, como os outros campos do
`settings.yaml`. Se o arquivo ficar inválido com o servidor já rodando, a janela abre com o tema e
a cor padrão em vez de deixar de abrir.
