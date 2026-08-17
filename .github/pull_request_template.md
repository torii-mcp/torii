## O que muda

<!-- Descreva o comportamento, não os arquivos. -->

## Contrato público

- [ ] não altera configuração pública, resposta MCP, CLI nem invariantes
- [ ] altera e a documentação em `docs/` foi atualizada no mesmo PR

## Invariantes

<!-- Cite os invariantes de docs/src/concepts/security-model.md que esta mudança toca
     e o teste que os cobre. Escreva "nenhum" quando for o caso. -->

## Verificação

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo test --all-targets`
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] `mdbook build docs`
