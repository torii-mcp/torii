# Fluxo de uma chamada

Considere a chamada à tool `aws`:

```json
{ "args": ["ec2", "describe-instances", "--region", "sa-east-1"] }
```

## 1. Dispatch MCP

O servidor confirma que a tool existe, rejeita campos extras e exige pelo menos um item em `args`.

Em provider target-aware, exige um alias conhecido, recusa flags bloqueadas e resolve context, paths e lock daquele target antes de qualquer leitura de ambiente.

## 2. Jasper

O provider é localizado e `rules.yaml` é carregado. Jasper verifica todos os denies antes dos accepts. O resultado é:

- `DeniedExplicit`: encerra sem credenciais ou processo;
- `Allowed`: registra a regra compatível;
- `Unresolved`: procura grant ativo e, se necessário, pede decisão humana.

## 3. Ambiente e sessão

Somente após `allow`, Torii lê o `.env` compartilhado e, quando houver, o `.env` do target. Em seguida, o lifecycle garante uma sessão válida no escopo.

## 4. Runner

O processo é construído conceitualmente assim:

```rust,ignore
Command::new(provider.command)
    .args(provider.args_prefix)
    .args(target.args_prefix)
    .args(request.args)
    .envs(persistent_env)
    .envs(auth_env);
```

Stdin do filho é nulo. Stdout e stderr são capturados. Um processo encerrado sem exit code recebe o fallback `143`.

## 5. Resultado

Torii devolve provider, target quando aplicável, decisão e, quando houve execução, exit code, stdout, stderr e indicador de truncamento. Exit code diferente de zero pertence ao provider e continua visível no resultado.

## Concorrência

Chamadas em escopos diferentes podem validar sessões independentemente. Chamadas concorrentes do mesmo provider simples ou target compartilham o lock daquele escopo.
