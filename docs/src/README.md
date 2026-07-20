# Torii

Torii é uma fronteira MCP local que permite a agentes executar ferramentas de infraestrutura sob uma política explícita e auditável.

```text
Cliente MCP
    |
    |  tool: kubectl
    |  target: mpce_dev
    |  args: ["get", "pods"]
    v
Torii -> target -> Jasper -> sessão isolada -> processo filho
```

O projeto separa dois mundos:

- humanos operam `aws`, `kubectl`, `az` e `gcloud` diretamente;
- agentes recebem tools MCP fornecidas pelo Torii;
- Jasper decide o que pode atravessar;
- o executável real continua responsável por validar sua gramática e permissões remotas.

## O que o Torii garante

- tudo começa negado;
- um `deny` explícito tem prioridade sobre qualquer permissão;
- argumentos permanecem estruturados e nunca passam por shell;
- políticas são avaliadas antes da leitura de credenciais;
- sessões coletadas pelo Torii são aplicadas somente ao processo filho;
- cada provider instalado vira exatamente uma tool MCP;
- targets são aliases cadastrados pelo humano, nunca contexts livres fornecidos pelo agente;
- aliases target-aware começam inativos e exigem um lease humano temporário antes de grants, ambiente ou autenticação;
- aliases `aws_profile` fixam profile e conta esperada fora do MCP e conferem a conta antes da execução;
- decisões e exit codes são auditados sem registrar credenciais.

## O que o Torii não é

Torii não é um novo AWS CLI, um parser completo de Kubernetes, um daemon multiusuário ou um catálogo de todas as operações de nuvem. Também não oferece ao agente tools para editar políticas, instalar providers, renovar credenciais ou desligar o servidor.

## Estado da implementação

A estratégia de autenticação `environment` está implementada e atende sessões temporárias AWS. `inherited` está implementada para providers que usam o ambiente ou credential store já isolado pelo operador; o modo `aws_profile` a combina com alias humano, remoção de overrides e verificação STS de conta. Os nomes `session_command` e `credential_file` fazem parte do schema, mas são recusados em runtime até que providers reais justifiquem suas implementações.

Para colocar o servidor em funcionamento, siga [Instalação](getting-started/installation.md) e [Primeiros passos](getting-started/quickstart.md). Para entender as garantias antes de operar em um ambiente sensível, leia o [Modelo de segurança](concepts/security-model.md).
