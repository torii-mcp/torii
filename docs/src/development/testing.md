# Testes e qualidade

Antes de entregar uma mudança:

```powershell
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
mdbook build docs
```

## Testes existentes

Testes unitários cobrem parsing de ambiente, matching, grants, pacotes em diretório/archive, setup, update preservando estado e truncamento UTF-8. `tests/security_flow.rs` prova que deny explícito e default deny headless encerram antes de ambiente, autenticação ou executável. `tests/mcp_readonly_integration.rs` negocia MCP com um processo Torii real, executa uma operação local de leitura permitida e confirma que outra leitura explicitamente negada não inicia o provider.

## Regressões prioritárias

Toda mudança no fluxo deve preservar testes para:

- deny vencendo accept;
- `s3` não casando com `s3api`;
- grant `exact` recusando argumento acrescentado, removido ou alterado;
- grant `prefix` permitindo somente as variações após a fronteira escolhida;
- credenciais não carregadas no caminho negado;
- argumentos encaminhados sem shell;
- exit code e streams preservados;
- reauth inválido mantendo sessão antiga;
- concorrência abrindo uma única coleta por escopo;
- target obrigatório, desconhecido e flags de override recusadas antes de env/auth;
- alias `aws_profile` mantendo profile e conta fora do schema MCP, removendo overrides e interrompendo antes do comando quando a identidade não confere;
- target-aware inativo por padrão, embora todos os aliases permaneçam no enum MCP;
- deny explícito em target inativo sem prompt de lease, grant, ambiente, autenticação ou processo;
- lease ativo antes de grants/env/auth, expiração e revogação bloqueando a chamada nas rechecagens antes de sessão e launch;
- `Replace`, `Add`, `Deny`, limites de 1 a 1.440 minutos, headless deny e CAS rejeitando uma decisão de janela obsoleta;
- digest de binding invalidando lease após editar, remover ou recriar um alias;
- `target clear` preservando grants, `.env`, cache, credenciais, `target.yaml` e processos já iniciados;
- múltiplos aliases ativos permanecendo no schema; na janela, o alerta ocupa a largura disponível junto às ações e **Adicionar** só conclui depois de 2 segundos de pressão contínua, reiniciando ao soltar; o clique que traz uma janela sem foco para frente já inicia a contagem;
- limite combinado de saída;
- tools/list contendo uma tool por provider e nenhuma tool de controle.
- pacote recusando rules base não vazio, setup recusando overwrite e update preservando rules/estado.

## Testar documentação

`mdbook build docs` valida `SUMMARY.md`, capítulos ausentes e links processados pelo preprocessor padrão. Blocos Rust meramente ilustrativos devem usar `rust,ignore`; exemplos compiláveis podem ser exercitados com `mdbook test docs` quando não dependerem do crate interno.

## Headless

Defina `TORII_NO_GUI=1` em automação. Não tente automatizar cliques em janelas como substituto para testes das regras e do lifecycle.
