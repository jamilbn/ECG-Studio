# Engenharia reversa do protocolo CONTEC8000G no ECG Workstation 3.4.8

Escopo: analise estatica do binario `vendor/contec/ECG Workstation V3.4.8/ECG Workstation V3.4.8.exe`, sem executar o software oficial e sem enviar comandos para aparelho real.

Status: protocolo parcial. A parte de enumeracao, casos salvos e deteccao do CONTEC8000G esta razoavelmente clara. A aquisicao ECG ao vivo ainda precisa de captura USB/serial/rede para validar frames de waveform.

## Artefatos analisados

- `ECG Workstation V3.4.8.exe`: PE32, timestamp `2024-04-21 21:32:59`, versao `3.4.8.0`.
- `Sync_CaseDownload.dll`: exporta funcoes de download de casos, identica entre 3.4.7 e 3.4.8.
- `CP2101.dll`: biblioteca Silicon Labs antiga para identificacao/configuracao CP2101; usada pelo EXE por ordinais para ler PID/product string, nao parece conter o protocolo ECG em si.

## Transportes

O EXE tem pelo menos tres caminhos de transporte:

- HID: comandos em output report com `report[0] = 0`, `report[1] = cmd`; usa `WriteFile` e aguarda evento de resposta.
- Serial/COM: usa `CreateFile`, `ReadFile`, `WriteFile`, `SetCommState`, `PurgeComm`.
- WLS/rede: usa `send`/`recv` em socket. O modo global observado `A406CC == 2` seleciona esse caminho.

No caminho mais provavel para CONTEC8000G/ECGXXXG, o codigo usa um handle em offset interno `+0x120808` e tenta portas seriais em `230400` (`0x38400`) e `460800` (`0x70800`) bps.

## Codificacao comum

Inteiros de indice/contagem usam dois bytes de 7 bits:

```cpp
uint8_t hi = (value >> 7) & 0x7F;
uint8_t lo = value & 0x7F;
uint16_t value = ((hi & 0x7F) << 7) | (lo & 0x7F);
```

Blocos binarios usam empacotamento com bytes de mascara. A rotina recuperada fica em `0x006EF3D6`:

```cpp
size_t UnpackContecBlock(uint8_t* dst, const uint8_t* src, size_t decodedLen) {
    const size_t maskLen = (decodedLen + 6) / 7;

    for (size_t i = 0; i < decodedLen; ++i) {
        const uint8_t low = src[maskLen + i];
        const uint8_t mask = src[i / 7];
        const uint8_t high = (mask << (7 - (i % 7))) & 0x80;
        dst[i] = low | high;
    }

    return maskLen + decodedLen;
}
```

Assim, um bloco decodificado de `N` bytes ocupa `N + ceil(N / 7)` bytes no fio.

## Comandos recuperados

| Comando | Uso observado | Observacao |
| --- | --- | --- |
| `0x80` | Deteccao CONTEC8000G/ECGXXXG | Espera resposta `0xF0` com 7 bytes. |
| `0x82` | Info via HID | Resposta HID `[0]=0, [1]=0x82`; bytes 4..9 formam id do dispositivo. |
| `0x83` | Lista/info ou ACK estendido | Em casos salvos: resposta `0x83` + tamanho + bloco empacotado. Em deteccao 8000G: espera frame `0xF3` de 17 bytes. |
| `0x86` | Contagem de casos | Serial/WLS espera `0x86` e depois dois bytes 7-bit. HID responde `[0]=0, [1]=0x80, [2]=case_count`. |
| `0x87` | Download de caso por indice | Envia `0x87`, `idx_hi`, `idx_lo`; espera `0x87`, tamanho e bloco empacotado. |
| `0x88` | Download/transferencia HID | Usa parametro em `report[2]` e thread auxiliar. Ainda nao fechado. |
| `0x8A` | Selecionar caso | Envia `0x8A`, `idx_hi`, `idx_lo`; espera `0x8A`. |
| `0x8B` | Apagar caso selecionado | Perigoso. Nao implementar no app por enquanto. |
| `0x8C` | Apagar todos / ack de delete | Perigoso. Nao implementar no app por enquanto. |
| `0x8D` | Confirmar delete | Perigoso. Nao implementar no app por enquanto. |
| `0x8F` | Finalizar/abortar transferencia | Usado ao fim de download de casos. |
| `0x90 0x00` | Sequencia de inicializacao 8000G | Enviado antes de `0x80` no detector CONTEC8000G. |
| `0x91` | Encerrar/fechar sessao serial | Espera `0x91`; tambem aparece no fechamento. |
| `0x93` | Consulta de tamanho/contador | Envia `0x93`; resposta contem `0x93`, depois dois bytes 7-bit. |
| `0x9D` | Ping/keepalive serial/WLS | Envia `0x9D`; espera `0x9D`. |
| `0xBA 0x02` | Pre-fechamento serial | Espera `0xFA 0x02` em caminho observado. |
| `0xC2 ... checksum` | Frame de preparacao 8000G | Frame de 10 bytes; checksum calculado por rotina em `0x006F1833`. Ainda precisa fechar formato exato. |
| `0xF0` | Resposta de deteccao 8000G | Frame de 7 bytes recebido depois de `0x80`. |
| `0xF3` | Resposta/ACK estendido | Frame de 17 bytes recebido depois de `0x83`. |

## Fluxo de casos salvos

### Listar casos

1. Enviar `0x83`.
2. Esperar eco/ACK `0x83`.
3. Ler 1 byte `len`, que deve ser `< 0x80`.
4. Ler `len + ceil(len / 7)` bytes.
5. Desempacotar com `UnpackContecBlock`.
6. Enviar `0x86`.
7. Esperar `0x86`.
8. Ler dois bytes 7-bit com a contagem de casos.

O payload desempacotado parece ser texto com pares separados por `;`, usando chaves numericas/`str`.

### Baixar caso

Para cada indice:

1. Enviar `0x87`.
2. Enviar indice em dois bytes 7-bit.
3. Esperar `0x87`.
4. Ler tamanho e bloco empacotado.
5. Desempacotar e processar.
6. Ao fim/sem dados, enviar `0x8F`.

### Selecionar caso

```text
TX: 8A idx_hi idx_lo
RX: 8A
```

### Comandos perigosos

O binario contem rotinas para apagar caso e apagar todos os casos:

```text
8B -> espera 8C -> 8D -> espera 8D
8C -> espera 8C -> 8D -> espera 8D
```

Esses comandos nao devem ser chamados pelo nosso app ate termos uma camada explicita de seguranca e permissao do usuario.

## Fluxo de deteccao CONTEC8000G/ECGXXXG

O detector de `CONTEC8000G`, `CONTEC8000GW`, `CONTEC8100G`, `CONTEC8100GW` fica na regiao `0x007648F0..0x00765890`.

Sequencia observada:

1. Abrir a porta candidata (`\\.\COMx`) em `230400` ou `460800` bps.
2. Verificar descricoes contendo `ecg`, `workstation`, `contec`, `8000` ou `CONTEC8000`.
3. Enviar um frame de 10 bytes iniciado por `0xC2`, com checksum.
4. Aguardar aproximadamente 400 ms.
5. Enviar `0x90 0x00`.
6. Drenar dados pendentes.
7. Enviar `0x80`.
8. Aguardar disponibilidade de 7 bytes.
9. Ler 7 bytes e exigir `frame[0] == 0xF0`.

Frame `0xF0` observado:

```text
byte 0: 0xF0
byte 1: codigo do modelo
byte 2: nao identificado
byte 3: nao identificado
byte 4: ano offset 2000
byte 5: mes
byte 6: dia
```

Mapeamento do `byte 1`:

| Codigo | Modelo |
| --- | --- |
| `0x03` | `CONTEC8000G` |
| `0x06` | `CONTEC8000GW` |
| `0x07` | `CONTEC8100G` |
| `0x08` | `CONTEC8100GW` |

Se a data do firmware for maior que `20160401`, o detector envia `0x83`, aguarda 17 bytes, exige `frame[0] == 0xF3` e desempacota 8 bytes a partir de `frame[1..]`. O codigo oficial parece esperar 8 bytes `0xFF` como ACK/estado.

## Consulta `0x93`

Rotina em `0x0075E491`:

1. Purga/limpa a porta.
2. Envia `0x93`.
3. Le ate 10 bytes.
4. Procura `0x93`.
5. Confirma que os dois bytes seguintes nao tem bit alto.
6. Retorna `((b1 & 0x7F) << 7) | (b2 & 0x7F)`.

## O que ainda falta para aquisicao ao vivo

Uma primeira captura real do ECG Workstation 3.4.8 com o aparelho conectado foi
salva em:

```text
captures/contec8000g/sessions/20260509-132959-contec8000g-unknown/raw/01_detect_open_workstation_usbpcap3_cp210x.pcapng
```

Essa captura confirma que o aparelho presente nesta bancada aparece como ponte
Silicon Labs CP210x (`10C4:EA60`, `COM6`) e que o Workstation configura a ponte
em `230400` bps antes de enviar bytes do protocolo serial. Os bytes seriais
trafegam por USB bulk OUT `0x02` e IN `0x82`.

Sequencia de live observada nessa captura:

```text
TX: 90 00
TX: 85 01
TX: 90 05
RX: EE 10 A0 49 ... fluxo continuo ...
TX: 90 00
RX: ... EE 10
```

Hipotese atual: `90 05` inicia a aquisicao/stream e o `90 00` final para ou
retorna o equipamento ao estado ocioso. O comando `85 01` ainda nao esta
mapeado. A captura contem aproximadamente `27.224` s de stream, `6386` pacotes
IN nao vazios e `408672` bytes de payload recebido.

Estrutura preliminar do stream:

- `EE 10` aparece como delimitador de inicio/fim do bloco recebido.
- O corpo contem registros candidatos `A0` de 15 bytes.
- Foram contados `27223` registros `A0`, muito proximo de `1000` registros/s.
- Existem `54` registros candidatos `B0` de 6 bytes, aproximadamente um a cada
  `0.5` s.
- Pacotes USB bulk de 64 bytes sao apenas chunks de transporte; o parser deve
  remontar o stream antes de interpretar registros.

Revisao com a captura `captures/contec8000g/8000G.pcapng` e os arquivos salvos
pelo Workstation em `captures/contec8000g/JamilCAP8000G`:

- `A0` e o marcador do registro, nao o subtipo clinico.
- Cada registro `A0` tem 15 bytes: `A0`, dois bytes de mascara e doze bytes de
  payload.
- Os doze bytes de payload formam oito ADCs de 12 bits: em cada grupo de tres
  bytes, o primeiro byte carrega os nibbles altos de dois canais e os dois
  bytes seguintes carregam os bytes baixos.
- A revalidacao com `ecg.c8k` e `tmpecg.c8k` salvos pelo Workstation indica que
  os dois canais de membro independentes sao `I` e `II`; `III`, `aVR`, `aVL` e
  `aVF` sao derivados deles.
- O stream `A0` chega em taxa aproximada de `1000` registros/s. O app reduz
  para `500 Hz` por media de pares de registros, compativel com os arquivos
  `tmpecg.c8k` gerados pelo Workstation.
- A taxa resultante para a captura salva e `500 Hz`, coerente com os arquivos
  `tmpecg.c8k` gerados pelo Workstation.
- `B0 00 00 ...` permanece como status periodico de 6 bytes e nao deve avancar
  o ciclo de canais.

Ainda existe uma anomalia unica na captura em que a distancia entre dois
marcadores `A0` e 24 bytes. O parser atual consome os 15 bytes confirmados e
descarta os 9 bytes extras ate o proximo marcador, preservando o ciclo de
canais.

Ainda nao ha evidencias suficientes para afirmar o formato completo dos frames de waveform em tempo real. O binario tem varias classes de visualizacao/analise (`CWaveView`, `CChanWaveView`, `CWaveDoc`, etc.), mas a parte encontrada ate agora para `CONTEC8000G` cobre principalmente:

- deteccao do equipamento;
- inicializacao de porta;
- lista/download de casos salvos;
- ACKs e fechamento.

Para fechar aquisicao ao vivo, precisamos de uma captura real do Workstation 3.4.8 conversando com o 8000G em modo de aquisicao, idealmente com sinal de teste, nao paciente.

O procedimento operacional de captura fica em:

```text
docs/protocols/contec8000g_capture.md
```

Captura minima desejada:

1. Conectar o 8000G.
2. Confirmar se aparece como HID, CP210x COM ou rede/WLS.
3. Rodar o Workstation 3.4.8.
4. Capturar desde conectar/iniciar exame ate alguns segundos de waveform.
5. Repetir encerrando exame normalmente.

Depois disso da para validar:

- baud rate/transporte real;
- comandos de iniciar/parar aquisicao;
- tamanho e sincronismo dos frames;
- escala ADC/mV;
- ordem dos leads;
- taxa de amostragem;
- checksum/CRC.

## Direcao segura para implementacao

Seguindo SOLID/DRY/YAGNI, a implementacao deve comecar read-only:

- `IContecTransport`: `Read`, `Write`, `Purge`, `Close`.
- `SerialContecTransport`, `HidContecTransport`, `SocketContecTransport`.
- `ContecPacketCodec`: 7-bit ints, `UnpackContecBlock`, checksums.
- `Contec8000GDevice`: apenas deteccao, listar casos, baixar caso.
- Bloquear `0x8B`, `0x8C`, `0x8D` por design.

Com a primeira captura validada, o streaming ao vivo entrou como prototipo
experimental, mantendo a decodificacao isolada para refinamento.

## Implementacao Rust inicial

O app Rust agora inclui uma captura live experimental baseada na sessao real
`20260509-132959-contec8000g-unknown`:

- abre a porta COM informada em `230400` bps;
- envia `90 00`, `85 01`, `90 05` para iniciar;
- interpreta o stream delimitado por `EE 10`;
- aceita registros `A0` de 15 bytes como oito ADCs de 12 bits compactados;
- calcula as 12 derivacoes clinicas a partir dos oito canais armazenados e
  emite uma amostra a 500 Hz por media de pares;
- ignora registros `B0` de status;
- envia `90 00` ao parar;
- preserva a captura em memoria para exportar HL7 aECG ou DICOM-ECG.

A decodificacao de waveform permanece conservadora: o app nomeia 12 derivacoes
padrao (`I`, `II`, `III`, `aVR`, `aVL`, `aVF`, `V1`-`V6`) a partir dos oito
canais armazenados (`I`, `II`, `V1`-`V6`), mas ainda e necessario capturar um
simulador ou sinal conhecido para confirmar escala clinica e o restante dos
campos do registro antes de uso diagnostico.
