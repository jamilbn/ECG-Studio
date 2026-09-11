# AGENTS.md

Behavioral guidelines to reduce common LLM coding mistakes. Merge with project-specific instructions as needed.

**Tradeoff:** These guidelines bias toward caution over speed. For trivial tasks, use judgment.

## 1. Think Before Coding

**Don't assume. Don't hide confusion. Surface tradeoffs.**

Before implementing:
- State your assumptions explicitly. If uncertain, ask.
- If multiple interpretations exist, present them - don't pick silently.
- If a simpler approach exists, say so. Push back when warranted.
- If something is unclear, stop. Name what's confusing. Ask.

## 2. Simplicity First

**Minimum code that solves the problem. Nothing speculative.**

- No features beyond what was asked.
- No abstractions for single-use code.
- No "flexibility" or "configurability" that wasn't requested.
- No error handling for impossible scenarios.
- If you write 200 lines and it could be 50, rewrite it.

Ask yourself: "Would a senior engineer say this is overcomplicated?" If yes, simplify.

## 3. Surgical Changes

**Touch only what you must. Clean up only your own mess.**

When editing existing code:
- Don't "improve" adjacent code, comments, or formatting.
- Don't refactor things that aren't broken.
- Match existing style, even if you'd do it differently.
- If you notice unrelated dead code, mention it - don't delete it.

When your changes create orphans:
- Remove imports/variables/functions that YOUR changes made unused.
- Don't remove pre-existing dead code unless asked.

The test: Every changed line should trace directly to the user's request.

## 4. Goal-Driven Execution

**Define success criteria. Loop until verified.**

Transform tasks into verifiable goals:
- "Add validation" → "Write tests for invalid inputs, then make them pass"
- "Fix the bug" → "Write a test that reproduces it, then make it pass"
- "Refactor X" → "Ensure tests pass before and after"

For multi-step tasks, state a brief plan:
```
1. [Step] → verify: [check]
2. [Step] → verify: [check]
3. [Step] → verify: [check]
```

Strong success criteria let you loop independently. Weak criteria ("make it work") require constant clarification.

---

**These guidelines are working if:** fewer unnecessary changes in diffs, fewer rewrites due to overcomplication, and clarifying questions come before implementation rather than after mistakes.

## Objetivo
Este projeto agora e um aplicativo desktop em Rust com interface Slint para leitura,
processamento, visualizacao e exportacao de ECG.

O programa deve:
- Ler arquivos ECG (`.xml`, `.aecg`, `.hl7`, `.c8k`, `.ecg`).
- Processar sinais e metadados de paciente/exame.
- Aplicar filtros simples de sinal.
- Renderizar uma pre-visualizacao em formato de pagina.
- Exportar um resumo textual do ECG carregado.

## Principios

### Simplicidade primeiro
- Prefira funcoes pequenas, nomes claros e modulos com responsabilidade unica.
- Evite abstracoes genericas antes de existir duplicacao real.
- Mantenha o dominio independente da UI sempre que possivel.

### Rust idiomatico
- Use `Result<T, E>` para erros recuperaveis.
- Evite `unwrap()` e `expect()` fora de inicializacao ou testes.
- Use slices (`&[T]`) quando nao houver necessidade de posse.
- Clone somente quando a copia for pequena ou simplificar claramente o fluxo.
- Preserve dados de ECG em `Vec<f64>` e reserve capacidade quando o tamanho for conhecido.

### Slint enxuto
- `ui/app.slint` deve conter layout declarativo e estado visual simples.
- Regras de negocio, parsing e renderizacao ficam em Rust.
- A UI chama callbacks; callbacks delegam para modulos de aplicacao.
- Evite colocar logica de ECG dentro do arquivo `.slint`.

### SOLID sem exagero
- SRP: leitores, processamento, preview e formatacao em modulos separados.
- OCP: novos leitores devem entrar como funcoes/modulos isolados e serem ligados em
  `readers::read_ecg`.
- DIP: UI depende do dominio exposto por Rust, nao de detalhes dos formatos.
- ISP/LSP: nao crie traits antes de haver mais de uma implementacao real com contrato
  estavel.

## Estrutura Recomendada

```text
/
  Cargo.toml
  build.rs
  ui/
    app.slint
  src/
    main.rs
    domain.rs
    formatting.rs
    preview.rs
    processing.rs
    readers/
      mod.rs
      c8k.rs
      contec.rs
      xml.rs
```

## Responsabilidades

- `domain.rs`: tipos centrais (`EcgDocument`, `LeadData`, `DocumentKind`).
- `readers/`: leitura e parsing de formatos de entrada.
- `processing.rs`: filtros de linha de base, baixa passagem e notch.
- `preview.rs`: rasterizacao da pagina de ECG para imagem Slint.
- `formatting.rs`: resumo textual/exportacao.
- `main.rs`: ligacao entre Slint, dialogs, estado da aplicacao e modulos.

## Performance

- Evite I/O repetido: leia cada arquivo apenas quando necessario.
- Use `Vec::with_capacity`/`reserve` quando o numero de amostras for previsivel.
- Renderize a preview em bitmap somente quando arquivo, metadados, filtro ou orientacao mudarem.
- Nao carregue binarios vendor nem instaladores dentro da base ativa do projeto.

## Fluxo do Programa

1. Usuario escolhe um arquivo.
2. `readers::read_ecg` detecta formato e retorna `EcgDocument`.
3. UI permite ajustar cabecalho e filtro.
4. `processing` gera uma copia filtrada quando necessario.
5. `preview` renderiza imagem de pagina para Slint.
6. `formatting` exporta resumo textual quando solicitado.

## Comandos

```powershell
cargo fmt
cargo test
cargo check
cargo run
cargo build --release
```

Se o workspace estiver no OneDrive e o Cargo encontrar `Access is denied` em `target/`,
use um diretorio de build fora da pasta sincronizada:

```powershell
$env:CARGO_TARGET_DIR = Join-Path $env:LOCALAPPDATA 'ECGStudio\target'
cargo check
cargo test
```

## Criterios de Aceitacao

- `cargo fmt` sem alteracoes pendentes.
- `cargo test` passa.
- `cargo check` passa.
- UI abre com Slint e permite carregar ECG.
- O ZIP legado permanece salvo na raiz do projeto.

## Restricoes

- Sem CMake, Visual Studio ou codigo Win32/GDI na base ativa.
- Sem vendor binario dentro do projeto ativo.
- Sem dependencias externas sem funcao clara.
- O projeto legado deve ficar apenas no arquivo `.zip` de backup.

## Filosofia

- Claro > esperto.
- Modular > espalhado.
- Rapido o suficiente > excessivamente generico.
