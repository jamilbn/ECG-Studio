# Ambiente de captura USB do CONTEC ECG90A

Escopo: preparar capturas reais do ECG90A para descobrir transporte, comandos e
frames de waveform. A documentacao abaixo e independente de linguagem: os mesmos
artefatos podem alimentar um leitor em Rust, Python, C, Go ou qualquer outro
ambiente.

Base: `contec8000g_protocol_re_3_4_8.md`. O ECG90A e da mesma familia CONTEC,
mas nenhum comando do 8000G deve ser tratado como confirmado para o ECG90A ate
aparecer em captura real.

## Objetivos

- Identificar como o ECG90A aparece no Windows: HID, porta COM USB/CP210x,
  CDC-ACM, armazenamento, bulk vendor-specific ou outro modo.
- Registrar descritores USB, VID/PID, interfaces, endpoints e drivers ativos.
- Capturar a conversa entre o software oficial e o aparelho desde a deteccao ate
  alguns segundos de sinal.
- Produzir logs normalizados de pacotes, com payload em hexadecimal e metadados
  suficientes para implementar depois em Rust sem depender do software oficial.
- Manter dados brutos fora do controle de versao por padrao.

## Regras de seguranca

- Use sinal de teste, simulador ou aparelho sem paciente conectado quando
  possivel.
- Nao capture dados identificaveis de paciente.
- Nao envie comandos manuais ao aparelho nesta fase. O ambiente e de observacao.
- Nao executar comandos de apagamento conhecidos da familia 8000G:
  `8B`, `8C`, `8D`.
- Guarde PCAP/PCAPNG e logs brutos apenas dentro de `captures/ecg90a/sessions/`,
  que e ignorado pelo Git.

## Hipoteses herdadas do 8000G

O protocolo do 8000G mostra tres caminhos possiveis:

| Transporte | Evidencia no 8000G | O que procurar no ECG90A |
| --- | --- | --- |
| HID | Output report com `report[0] = 0`, `report[1] = cmd` | Interface USB class `03`, reports pequenos e endpoints interrupt. |
| Serial/COM | `CreateFile`, `ReadFile`, `WriteFile`, `230400` ou `460800` bps | Porta `COMx`, CP210x/CDC, fluxo 8N1. |
| WLS/rede | `send`/`recv` em socket | Nao e prioridade no ECG90A USB, mas anotar se aparecer adaptador/rede. |

Comandos observados no 8000G que servem apenas como marcadores de busca:

| Bytes | Significado no 8000G | Status para ECG90A |
| --- | --- | --- |
| `80` | Deteccao; resposta `F0` | Hipotese. |
| `83` | Lista/info ou ACK estendido | Hipotese. |
| `86` | Contagem de casos | Hipotese. |
| `87` | Download de caso | Hipotese. |
| `8A` | Selecionar caso | Hipotese. |
| `8F` | Finalizar transferencia | Hipotese. |
| `90 00` | Inicializacao 8000G | Hipotese. |
| `91` | Fechar sessao serial | Hipotese. |
| `93` | Consulta de tamanho/contador | Hipotese. |
| `9D` | Ping/keepalive | Hipotese. |
| `C2 ...` | Preparacao com checksum | Hipotese. |

## Estrutura de uma sessao

Crie uma sessao com:

```powershell
.\tools\ecg90a_capture\New-Ecg90aCaptureSession.ps1 -Transport usbpcap -Operator "lab"
```

Se a politica local bloquear `.ps1`, use:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File `
  .\tools\ecg90a_capture\New-Ecg90aCaptureSession.ps1 `
  -Transport usbpcap -Operator "lab"
```

A pasta gerada segue este formato:

```text
captures/ecg90a/sessions/YYYYMMDD-HHMMSS-ecg90a-TRANSPORT/
  manifest.md
  usb_inventory.csv
  raw/
    *.pcapng
  exports/
    *.csv
  analysis/
    frames.csv
    findings.md
  notes/
    observations.md
```

Campos minimos do `manifest.md`:

| Campo | Descricao |
| --- | --- |
| Device | Modelo fisico observado no gabinete/tela. |
| Serial number | Numero de serie se estiver visivel; pode ficar vazio. |
| Firmware | Versao mostrada no aparelho ou no software oficial. |
| Host OS | Windows/build usado na captura. |
| Official software | Nome e versao do software usado para gerar trafego. |
| Transport | `usbpcap`, `hid`, `serial`, `bulk`, `unknown`. |
| VID/PID | Extraido do inventario USB. |
| Scenario | Deteccao, aquisicao ao vivo, download de caso, encerramento. |
| Signal source | Simulador, sinal de teste interno ou observacao sem paciente. |

## Inventario USB

Antes de abrir o software oficial, conecte o ECG90A e rode:

```powershell
.\tools\ecg90a_capture\Export-Ecg90aUsbInventory.ps1 `
  -OutFile .\captures\ecg90a\sessions\<sessao>\usb_inventory.csv
```

Procure por:

- `VID_####` e `PID_####`;
- fabricante contendo `CONTEC`, `Silicon`, `CP210`, `USB Serial` ou similar;
- classe `Ports`, `HIDClass`, `USB` ou driver vendor-specific;
- criacao de uma porta `COMx` quando o aparelho e conectado.

## Capturas minimas

Cada etapa deve gerar um PCAPNG separado ou marcadores claros no mesmo arquivo.

| Nome sugerido | Acao | Duracao |
| --- | --- | --- |
| `01_plugin_idle.pcapng` | Conectar o ECG90A sem abrir o software oficial. | 10 s |
| `02_official_detect.pcapng` | Abrir o software oficial e deixar detectar o aparelho. | 20 s |
| `03_live_start_10s.pcapng` | Iniciar aquisicao/preview e manter sinal estavel. | 10 s |
| `04_live_stop.pcapng` | Encerrar aquisicao normalmente. | 10 s |
| `05_case_list_download.pcapng` | Listar/baixar caso salvo, se existir e for seguro. | opcional |

Anote em `notes/observations.md` o instante aproximado de cada acao humana:
conectar, abrir software, clicar em iniciar, aparecer waveform, clicar em parar,
fechar software.

## Formato universal de pacotes normalizados

O arquivo `analysis/frames.csv` usa colunas estaveis e independentes de Rust:

```text
timestamp_utc,direction,transport,interface,endpoint,transfer_type,command,length,payload_hex,decoded_hint,notes
```

Definicoes:

| Coluna | Valor esperado |
| --- | --- |
| `timestamp_utc` | ISO-8601 UTC, por exemplo `2026-05-08T18:30:22.123456Z`. |
| `direction` | `host_to_device`, `device_to_host` ou `unknown`. |
| `transport` | `hid`, `serial`, `usb_control`, `usb_interrupt`, `usb_bulk`, `unknown`. |
| `interface` | Numero da interface USB ou porta COM. |
| `endpoint` | Endpoint USB, report ID ou vazio para serial puro. |
| `transfer_type` | `control`, `interrupt`, `bulk`, `isochronous`, `serial`. |
| `command` | Primeiro byte/bytes provaveis de comando, em hex; vazio se desconhecido. |
| `length` | Quantidade de bytes do payload. |
| `payload_hex` | Bytes em hexadecimal, separados por espaco, sem prefixo `0x`. |
| `decoded_hint` | Interpretacao humana curta, sempre marcada como hipotese quando nao validada. |
| `notes` | Observacoes livres. |

Regras para `payload_hex`:

- usar maiusculas: `90 00`, `F0 03 00 00 18 05 08`;
- separar bytes com um espaco;
- nao misturar decimal, base64 ou literais de linguagem.

## Exportacao com Wireshark/tshark

Quando USBPcap/Wireshark estiver instalado, exporte campos basicos para CSV:

```powershell
tshark -r .\raw\03_live_start_10s.pcapng `
  -Y "usb.capdata || usbhid.data" `
  -T fields -E header=y -E separator=, -E quote=d `
  -e frame.time_epoch `
  -e usb.src `
  -e usb.dst `
  -e usb.transfer_type `
  -e usb.endpoint_address `
  -e usb.capdata `
  -e usbhid.data `
  > .\exports\03_live_start_10s_usb_fields.csv
```

Depois converta manualmente ou por script para `analysis/frames.csv`. A primeira
rodada pode ser manual: o objetivo e descobrir padroes, nao automatizar cedo.

## Captura HID read-only sem USBPcap

Se o Windows expuser o ECG90A como `HIDClass`, e USBPcap/Wireshark nao estiverem
instalados, use a ferramenta local de leitura de input reports:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File `
  .\tools\ecg90a_capture\Capture-Ecg90aHidReports.ps1 `
  -VidPid 0483:5750 `
  -Seconds 10 `
  -OutFile .\captures\ecg90a\sessions\<sessao>\exports\hid_0483_5750.csv
```

Esse modo nao envia comandos ao aparelho. Ele tenta abrir o HID apenas para
leitura e grava cada input report no mesmo formato de `analysis/frames.csv`.
Se o software oficial abriu o HID de forma exclusiva, a captura pode falhar com
acesso negado; nesse caso o caminho correto e instalar USBPcap/Wireshark e
capturar no nivel USB.

## Achados iniciais do ECG90A

Captura de referencia: `captures/ecg90a/sessions/20260509-122452-ecg90a-usbpcap/raw/02_get_frequency_failed_usbpcap3_vid0483.pcapng`.

Dados confirmados nessa captura:

| Item | Valor |
| --- | --- |
| VID/PID | `0483:5750` |
| Windows instance | `USB\VID_0483&PID_5750\STM3210` |
| Transporte | USB HID |
| Interface | HID class `0x03`, subclass `0x00`, protocol `0x00` |
| Endpoint IN | `0x81`, interrupt, max packet `64`, interval `1` |
| Endpoint OUT | `0x02`, interrupt, max packet `8`, interval `16` |

Sequencias observadas:

| Payload | Direcao | Status | Observacao |
| --- | --- | --- | --- |
| `91 00 00 00 00 00 00 00` | host -> ECG90A, endpoint `0x02` | provavel | Candidato forte a comando de frequencia/estado; uma resposta observada foi `FF 06 00 ...`. |
| `82 00 00 00 00 00 00 00` | host -> ECG90A, endpoint `0x02` | confirmado como info/modelo | A resposta `82 64 00 45 43 47 39 30 41 ...` contem ASCII `ECG90A`. |

Interpretacao provisoria:

- Os comandos de aplicacao sao reports HID de 8 bytes no endpoint OUT `0x02`.
- As respostas chegam como reports HID de 64 bytes no endpoint IN `0x81`.
- O byte inicial parece ser o codigo de comando/resposta.
- `FF xx ...` parece report de ACK/estado/erro, mas o significado de `xx` ainda precisa de captura comparativa.
- Em uma unidade com comportamento suspeito de firmware corrompido, o envio do comando `91 00 00 00 00 00 00 00` fez o aparelho voltar ao menu principal e gerou resposta `FF 06 00 ...`; portanto `FF 06` deve ser tratado como possivel erro/estado invalido ate comparacao com uma unidade funcional.
- Para fechar o erro `Send getting frequency of device failed!`, precisamos comparar essa captura com uma captura equivalente em um computador onde o mesmo comando funciona.

## Checklist de analise

1. Confirmar transporte real pelo inventario USB.
2. Separar trafego de enumeracao USB do trafego de aplicacao.
3. Identificar quem fala primeiro depois que o software oficial abre.
4. Procurar bytes conhecidos da familia 8000G apenas como marcadores:
   `80`, `83`, `86`, `87`, `8F`, `90 00`, `91`, `93`, `9D`, `C2`.
5. Marcar cada sequencia como `confirmed`, `probable` ou `unknown`.
6. Localizar inicio/parada de waveform comparando timestamps com as anotacoes.
7. Medir tamanho de frames repetidos, intervalo entre frames e bytes de contador.
8. Verificar se ha checksum, CRC, contador monotono ou pacote de sync.
9. Estimar ordem de derivacoes, taxa de amostragem e escala so depois de obter
   sinal conhecido.

## Entrega esperada apos capturas

Para cada sessao util, guardar:

- `manifest.md` preenchido;
- `usb_inventory.csv`;
- PCAPNG bruto em `raw/`;
- CSV exportado em `exports/`;
- `analysis/frames.csv` com os pacotes mais relevantes;
- `analysis/findings.md` com hipoteses e evidencias.

So depois disso deve nascer o modulo Rust de acesso ao ECG90A. A primeira
implementacao no app deve ser read-only: detectar aparelho, iniciar captura,
parar captura e fechar sessao. Escrita de comandos destrutivos fica proibida por
design.
