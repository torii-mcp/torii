# Limites atuais e evolução

Esta página distingue deliberadamente o que existe do que ainda precisa de prova real.

## Implementado

- MCP stdio e lifecycle controlado pelo cliente;
- uma tool dinâmica por provider;
- Jasper com regras e grants;
- aprovação e autenticação GUI;
- auth `environment` e `inherited`;
- lock de renovação por provider simples ou target;
- runner capturado, sem shell e com truncamento;
- auditoria local;
- migração AWS Gate;
- exemplos AWS e Kubernetes;
- tool Kubernetes única com targets por context;
- CLI humana de targets e isolamento de grants/cache/auth por alias;
- leases humanos por provider para aliases target-aware, com expiração, digest de binding, CAS, lock e escrita atômica;
- pacotes declarativos locais/remotos, catálogo pesquisável e update que preserva estado;

## Reconhecido, não implementado

- `session_command`;
- `credential_file`.

O schema aceita esses nomes para manter a direção arquitetural explícita, mas runtime retorna erro. O primeiro provider real de Azure ou GCP deve orientar a implementação correta.

## Fora do escopo atual

- tool por operação;
- SDKs de nuvem substituindo os CLIs;
- parser completo de kubectl;
- assinatura criptográfica do catálogo/pacotes e remoção automatizada;
- distribuição OCI, WASM ou atualização automática;
- OAuth remoto;
- daemon multiusuário;
- tool MCP de reauth ou kill;
- CLI operacional;
- streaming e timeout de execução.

## Regra de evolução

Uma abstração nova deve resolver duplicação observada em pelo menos dois providers ou uma necessidade operacional comprovada. Simplicidade e explicabilidade são requisitos do produto, não uma etapa temporária.
