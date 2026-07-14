# AGENTS.md — documentação

Estas instruções complementam o `AGENTS.md` da raiz para tudo sob `docs/`.

- A documentação oficial é escrita em português brasileiro, com nomes de tipos, campos, comandos e protocolos preservados em inglês.
- Todo capítulo navegável deve estar em `src/SUMMARY.md`.
- `book.toml` usa `create-missing = false`; não dependa da criação automática de capítulos.
- Use links relativos entre capítulos e valide com `mdbook build docs`.
- Exemplos devem usar credenciais, contas, clusters e caminhos fictícios.
- Diferencie claramente comportamento implementado, recomendação operacional e roadmap.
- Não prometa streaming, timeout, instalação de provider, `session_command` ou `credential_file` enquanto o código não os implementar.
- Mudanças no schema de provider devem atualizar `reference/provider-schema.md` e ao menos um exemplo em `examples/providers/`.
- Mudanças na tool MCP devem atualizar `reference/mcp-api.md`.
- Mudanças no control plane devem atualizar `reference/cli.md`.
- Não edite `docs/book/`; ele é artefato gerado e ignorado pelo Git.

