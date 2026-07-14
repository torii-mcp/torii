# Documentação do Torii

Este diretório contém o livro oficial em mdBook.

```text
docs/
├── book.toml       configuração do livro
├── src/            capítulos versionados
├── theme/          customização visual
└── book/           HTML gerado, ignorado pelo Git
```

## Comandos

```powershell
mdbook build docs
mdbook test docs
mdbook serve docs --open
```

O índice vive em `src/SUMMARY.md`. O build usa `create-missing = false`, então todo capítulo adicionado ao sumário precisa existir no mesmo commit.

Consulte o `AGENTS.md` deste diretório antes de alterar conteúdo público.

