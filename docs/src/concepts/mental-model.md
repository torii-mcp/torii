# Modelo mental

A menor descrição correta do Torii é:

```text
servidor MCP local
+ registry pequeno de providers e targets
+ Jasper
+ controle humano local
+ sessões isoladas por escopo
+ runner sem shell
```

## Três fronteiras

### Data plane do agente

O agente lista tools e chama um provider com `args: string[]`. Em uma tool target-aware, escolhe apenas um alias anunciado pelo schema. Todos os aliases configurados são anunciados, mas só os que possuem lease humano válido podem atravessar para grants, ambiente e autenticação. Ele não cadastra ou ativa targets, não fornece contexts reais, não conhece credenciais e não controla configuração ou lifecycle.

### Control plane humano

O humano edita YAML, cadastra aliases, aprova chamadas não resolvidas e renova sessões. Os subcomandos locais existem apenas para esse controle.

### Autoridade externa

Mesmo quando Jasper permite uma tentativa, AWS IAM, Kubernetes RBAC ou a autoridade equivalente ainda decide se a identidade pode realizar a operação.

```text
Jasper: o agente pode tentar?
Cloud/cluster: esta identidade pode realizar?
```

## Escopos

Providers simples, como `aws`, isolam política e sessão na raiz do provider. Providers target-aware, como `kubectl`, compartilham o mecanismo e isolam política, grants e ambiente por alias:

```text
kubectl/mpce_dev
kubectl/cliente_hml
```

O alias é identidade de configuração, auditoria e grants; o context real permanece sob controle humano. O conjunto de leases é exceção deliberada: fica no escopo do provider para substituir ou adicionar aliases de forma atômica.

`aws_profile` usa o mesmo conceito de alias, mas o binding é profile local mais conta esperada. Esses valores permanecem no control plane; para o agente, `producao` é apenas o alias anunciado.

Um alias configurado não é automaticamente utilizável. O humano o ativa por um lease temporário; a ativação padrão substitui todos os aliases ativos da mesma tool. Na janela aberta por uma chamada, quando **Adicionar** criar múltiplos ativos, o alerta fica junto às ações e o humano mantém o botão pressionado por 2 segundos para confirmar que o agente poderá escolher qualquer alias ativo enquanto o lease durar. No CLI, `target activate --add` explicita a mesma escolha.
