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

O agente lista tools e chama um provider com `args: string[]`. Em uma tool target-aware, escolhe apenas um alias anunciado pelo schema. Ele não cadastra targets, não fornece contexts reais, não conhece credenciais e não controla configuração ou lifecycle.

### Control plane humano

O humano edita YAML, cadastra aliases, aprova chamadas não resolvidas e renova sessões. Os subcomandos locais existem apenas para esse controle.

### Autoridade externa

Mesmo quando Jasper permite uma tentativa, AWS IAM, Kubernetes RBAC ou a autoridade equivalente ainda decide se a identidade pode realizar a operação.

```text
Jasper: o agente pode tentar?
Cloud/cluster: esta identidade pode realizar?
```

## Escopos

Providers simples, como `aws`, isolam política e sessão na raiz do provider. Providers target-aware, como `kubectl`, compartilham o mecanismo e isolam estado por alias:

```text
kubectl/mpce_dev
kubectl/cliente_hml
```

O alias é identidade de configuração, auditoria e grants; o context real permanece sob controle humano.
