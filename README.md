# Vit Language — Documentação

Vit é uma linguagem compilada estaticamente tipada que gera binários nativos via LLVM.

---

## Índice

- [Tipos](#tipos)
- [Variáveis](#variáveis)
- [Variáveis globais](#variáveis-globais)
- [Literais](#literais)
- [Operadores](#operadores)
- [Controle de fluxo](#controle-de-fluxo)
- [Funções](#funções)
- [Entrada e Saída](#entrada-e-saída)
- [Arrays](#arrays)
- [Structs](#structs)
- [Funções built-in](#funções-built-in)
- [Funções de string](#funções-de-string)
- [HashMap](#hashmap)
- [Módulos e imports](#módulos-e-imports)
- [Funções externas (extern fn)](#funções-externas-extern-fn)
- [Stdlib](#stdlib)
- [Limitações conhecidas](#limitações-conhecidas)
- [Compilação e execução](#compilação-e-execução)

---

## Tipos

| Tipo      | Descrição                        | Exemplo            |
|-----------|----------------------------------|--------------------|
| `i32`     | Inteiro 32 bits com sinal        | `42`, `-7`         |
| `i64`     | Inteiro 64 bits com sinal        | `9999999999`       |
| `f64`     | Ponto flutuante 64 bits (double) | `3.14`, `0.5`      |
| `f32`     | Ponto flutuante 32 bits (float)  | via `input` apenas |
| `bool`    | Booleano                         | `true`, `false`    |
| `str`     | String                           | `"hello"`          |
| `[T; N]`  | Array de N elementos do tipo T   | `[i32; 5]`         |
| `NomeStruct` | Struct definida pelo usuário  | `Point { x: 1, y: 2 }` |

> **Nota:** Literais float (`3.14`) são sempre `f64`. Para `f32`, use `input`.

---

## Variáveis

### Declaração com valor inicial
```vit
let x: i32 = 42;
let pi: f64 = 3.14159;
let ativo: bool = true;
let nome: str = "Vit";
let nums: [i32; 3] = [1, 2, 3];
```

### Declaração sem valor inicial
```vit
let x: i32;       // valor indefinido até atribuição (como em C)
let arr: [f64; 4];
```

### Reatribuição
```vit
x = x + 1;
```

### Atribuição composta
```vit
x += 1;    // equivale a x = x + 1
x -= 5;
x *= 2;
x /= 3;
x %= 7;
```

---

## Variáveis globais

Declaradas fora de qualquer função. Visíveis em todas as funções. Inicializadas com zero por padrão ou com literal constante.

```vit
let MOD: i32 = 1000000007;
let dp: [i32; 1000];         // zero-inicializado
let contador: i64;

fn incrementa() -> i32 {
  contador += 1;
  return 0;
}

fn main() -> i32 {
  incrementa();
  print contador;   // 1
  print MOD;
  return 0;
}
```

> **Regras:**
> - Devem ser declaradas antes das funções
> - Inicializadores de globais devem ser literais constantes (não expressões)
> - Arrays globais são zero-inicializados se sem inicializador — útil para DP

---

## Literais

| Literal          | Tipo inferido | Exemplo             |
|------------------|---------------|---------------------|
| `42`             | `i32`         | inteiro             |
| `-7`             | `i32`         | negativo            |
| `3.14`           | `f64`         | float               |
| `true` / `false` | `bool`        | booleano            |
| `"texto"`        | `str`         | string              |
| `[1, 2, 3]`      | array         | só em inicializador |

### Escapes em strings
| Escape | Caractere       |
|--------|-----------------|
| `\n`   | Nova linha      |
| `\r`   | Carriage return |
| `\t`   | Tab             |
| `\\`   | Barra invertida |
| `\"`   | Aspas duplas    |

---

## Operadores

### Aritméticos
```vit
x + y    // soma
x - y    // subtração
x * y    // multiplicação
x / y    // divisão (inteira para i32/i64, real para f64)
x % y    // módulo / resto (inteiros apenas)
-x       // negação unária (i32, i64, f64)
```

### Comparação
```vit
x == y   // igual
x != y   // diferente
x < y    // menor que
x > y    // maior que
x <= y   // menor ou igual
x >= y   // maior ou igual
```

### Lógicos (apenas `bool`)
```vit
a && b   // E lógico
a || b   // OU lógico
!a       // negação lógica
```

### Bitwise (inteiros)
```vit
a & b    // AND bit a bit
a | b    // OR bit a bit
a ^ b    // XOR bit a bit
a << n   // shift left  (a * 2^n)
a >> n   // shift right aritmético (a / 2^n)
```

```vit
let par: bool = (n & 1) == 0;   // verifica paridade
let pot: i32  = 1 << 10;        // 1024
```

### Cast de tipo
Converte explicitamente entre tipos numéricos.

```vit
let big: i64 = x as i64;          // i32 → i64 (extensão de sinal)
let n: i32   = 3.7 as i32;        // f64 → i32 (trunca, não arredonda → 3)
let f: f64   = contador as f64;   // i32 → f64
```

### Precedência (menor → maior)
```
||  →  &&  →  == != < > <= >=  →  + -  →  * / % & | ^ << >>  →  - ! (unário)  →  as
```

---

## Controle de fluxo

### if / else if / else
```vit
if condicao {
  // then
} else if outra {
  // else if
} else {
  // else
}
```

```vit
if x < 0 {
  print "negativo";
} else if x == 0 {
  print "zero";
} else {
  print "positivo";
}
```

### while
```vit
while condicao {
  // corpo
}
```

### for
Itera de `inicio` (inclusive) até `fim` (exclusive), passo 1.

```vit
for i in 0..n {
  // i vai de 0 até n-1
}
```

### break / continue
```vit
for i in 0..n {
  if nums[i] == target {
    print i;
    break;          // sai do loop imediatamente
  }
  if nums[i] < 0 {
    continue;       // pula para a próxima iteração
  }
  print nums[i];
}
```

> `break` e `continue` funcionam em `for` e `while`. Em loops aninhados, afetam apenas o loop mais interno.

---

## Funções

### Definição
```vit
fn nome(param1: tipo1, param2: tipo2) -> tipo_retorno {
  return valor;
}
```

### Exemplos
```vit
fn soma(a: i32, b: i32) -> i32 {
  return a + b;
}

fn media(a: f64, b: f64) -> f64 {
  return (a + b) / 2.0;
}

fn fatorial(n: i32) -> i32 {
  if n <= 1 {
    return 1;
  } else {
    return n * fatorial(n - 1);
  }
}

fn saudacao(nome: str) -> str {
  return add("Ola, ", nome);
}
```

### Chamada como statement
Funções podem ser chamadas sem capturar o retorno:
```vit
sort(arr, n);       // resultado descartado
```

> **Regras:**
> - O programa precisa ter uma função `main` como ponto de entrada
> - Funções devem ser declaradas antes de `main` (ordem importa)
> - Funções podem receber e retornar `str`
> - Arrays são passados como ponteiro para o primeiro elemento (sem informação de tamanho)
> - Structs são passadas por ponteiro (cópia local na função receptora)

---

## Entrada e Saída

### print
Aceita múltiplos valores separados por vírgula. Quebra de linha automática no final.

```vit
print 42;
print "Resultado: ", x;
print "a = ", a, " b = ", b;
print x + y;
```

| Tipo   | Formato |
|--------|---------|
| `i32`  | `%d`    |
| `i64`  | `%ld`   |
| `f64` / `f32` | `%f` |
| `bool` | `0` ou `1` |
| `str`  | `%s`    |

### input
Declara e lê uma variável do stdin:

```vit
input x: i32;       // lê inteiro
input y: f64;       // lê float
input nome: str;    // lê linha inteira (até \n, máx 255 chars)
```

Lê diretamente em elemento de array (variável já declarada):

```vit
input arr[i];       // sem tipo — usa o tipo do array
```

> `scanf` pula whitespace automaticamente — `input` de i32/f64 lê valores separados por espaço ou newline indistintamente. Ideal para leitura de múltiplos valores por linha.

---

## Arrays

### Declaração
```vit
let arr: [i32; 5];                         // sem inicializar
let arr: [i32; 5] = [10, 20, 30, 40, 50]; // com inicializador
let notas: [f64; 3] = [7.5, 8.0, 9.5];
let partes: [str; 10];                     // array de strings (para split)
```

### Acesso e atribuição
```vit
print arr[0];
arr[i] = arr[i] + 1;
```

### Iteração
```vit
for i in 0..5 {
  print arr[i];
}
```

### Input de elementos
```vit
// Lê 5 inteiros — funciona com "1 2 3 4 5" ou um por linha
for i in 0..5 {
  input arr[i];
}
```

> **Limitações:** tamanho fixo em compile-time, sem bounds checking, sem arrays multidimensionais.

---

## Structs

Tipos compostos definidos pelo usuário. O nome deve começar com letra maiúscula.

### Definição

```vit
struct Point {
    x: i32,
    y: i32,
}

struct Pessoa {
    idade: i32,
    score: f64,
}
```

### Criação (literal)

```vit
let p: Point = Point { x: 10, y: 20 };
```

### Acesso a campos

```vit
print p.x;          // 10
print p.y;          // 20
let soma: i32 = p.x + p.y;
```

### Modificação de campos

```vit
p.x = 99;
p.y = p.y + 1;
```

### Structs em funções

Structs são passadas por ponteiro (cópia local na função receptora). O retorno por valor também é suportado.

```vit
fn distancia(p: Point) -> f64 {
    let fx: f64 = p.x as f64;
    let fy: f64 = p.y as f64;
    return sqrt(fx * fx + fy * fy);
}

fn make_point(x: i32, y: i32) -> Point {
    return Point { x: x, y: y };
}

fn main() -> i32 {
    let p: Point = make_point(3, 4);
    let d: f64 = distancia(p);
    print d;    // 5.0
    return 0;
}
```

### Tipos de campos suportados

| Tipo do campo   | Suportado |
|-----------------|-----------|
| `i32`, `i64`    | Sim       |
| `f32`, `f64`    | Sim       |
| `bool`          | Sim       |
| `str`           | Sim       |
| Struct aninhada | Sim       |
| `map[K, V]`     | Sim       |
| Array           | Não       |

---

## Funções built-in

### Matemáticas

#### abs(x) — valor absoluto
Funciona com `i32`, `i64`, `f64`.
```vit
let v: i32 = abs(-42);    // 42
let f: f64 = abs(-3.14);  // 3.14
```

#### min(a, b) / max(a, b)
Funciona com inteiros e floats do mesmo tipo.
```vit
let menor: i32 = min(a, b);
let maior: f64 = max(x, y);
```

#### sqrt(x) — raiz quadrada
Retorna `f64`. Aceita inteiros (auto-cast).
```vit
let r: f64 = sqrt(16);     // 4.0
let r: f64 = sqrt(2.0);    // 1.4142...
```

#### pow(base, exp) — potência
Retorna `f64`. Aceita inteiros (auto-cast).
```vit
let p: f64 = pow(2.0, 10.0);  // 1024.0
let p: f64 = pow(2, 8);       // 256.0
```

### Tamanho

#### len(x) — tamanho de array ou string
- Para arrays: retorna o tamanho em compile-time (`i32`)
- Para strings: chama `strlen` em runtime (`i32`)

```vit
let arr: [i32; 100];
let n: i32 = len(arr);     // 100 (compile-time)

input s: str;
let sl: i32 = len(s);      // strlen em runtime
```

### Ordenação

#### sort(arr, n) — ordena array in-place
Usa `qsort`. Funciona com `[i32; N]`, `[i64; N]`, `[f64; N]`. Ordem crescente.

```vit
let nums: [i32; 5] = [3, 1, 4, 1, 5];
sort(nums, 5);
print nums[0];   // 1
print nums[4];   // 5
```

```vit
// Ordenar apenas os primeiros n elementos lidos
input n: i32;
let arr: [i32; 1000];
for i in 0..n {
  input arr[i];
}
sort(arr, n);
```

### Conversão de tipos

#### str_to_int(s) → i32
```vit
let n: i32 = str_to_int("42");    // 42
```

#### str_to_float(s) → f64
```vit
let f: f64 = str_to_float("3.14");
```

#### int_to_str(n) → str
Funciona com `i32`, `i64`, `f64`.
```vit
let s: str = int_to_str(42);      // "42"
print "Valor: ", s;
```

#### format(fmt, ...) → str
Formata uma string usando a sintaxe do `printf` do C. Retorna um novo `str` alocado em memória.
```vit
let s: str = format("x = %d, y = %.2f", x, y);
let json: str = format("{\"nome\": \"%s\", \"idade\": %d}", nome, idade);
print s;
```

| Especificador | Tipo      |
|---------------|-----------|
| `%d`          | `i32`     |
| `%ld`         | `i64`     |
| `%f`          | `f64`     |
| `%s`          | `str`     |
| `%.2f`        | `f64` (2 casas decimais) |

> `format()` aloca 4096 bytes por chamada. Para strings maiores, use `sprintf` via `extern fn`.

---

## Funções de string

Funções built-in para manipulação de strings. Não modificam o original — retornam um novo `str`.

### add(s1, s2) — concatenação
```vit
let s: str = add("Ola, ", "mundo");   // "Ola, mundo"
```

### remove(s, sub) — remove primeira ocorrência
```vit
let s: str = remove("hello world", "world");   // "hello "
```
Se não encontrado, retorna o original.

### replace(s, old, new) — substitui primeira ocorrência
```vit
let s: str = replace("foo bar foo", "foo", "baz");   // "baz bar foo"
```
Se não encontrado, retorna o original.

### split(s, sep, arr) — divide em array
Preenche `arr` com as partes e retorna a quantidade encontrada.

```vit
input linha: str;              // ex: "15 20 25"
let partes: [str; 10];
let n: i32 = split(linha, " ", partes);
print partes[0];               // "15"
print partes[1];               // "20"
```

> - `arr` deve ser `[str; N]` — N é o limite máximo de partes
> - Separadores consecutivos geram tokens vazios (comportamento do `strtok`)
> - Split opera em cópia interna — o original não é modificado

### Exemplo completo
```vit
fn main() -> i32 {
  // Lê "10 20 30", converte e soma
  input linha: str;
  let partes: [str; 3];
  let n: i32 = split(linha, " ", partes);

  let soma: i32 = 0;
  for i in 0..n {
    soma += str_to_int(partes[i]);
  }
  print soma;   // 60

  return 0;
}
```

> As funções de string alocam memória com `malloc`. Sem GC — para programas curtos não é problema.

---

## HashMap

Tabela hash open-addressing com capacidade 4096. Alocada automaticamente com `calloc`.

### Declaração

```vit
let m: map[i32, i32];      // chave i32, valor i32
let m: map[i32, i64];      // chave i32, valor i64
let m: map[i64, i32];      // chave i64, valor i32
let m: map[i64, i64];      // chave i64, valor i64
let m: map[str, i32];      // chave string, valor i32
```

### API

#### map_set(m, chave, valor)
Insere ou atualiza uma entrada.
```vit
map_set(m, 42, 100);
map_set(freq, "hello", 1);
```

#### map_get(m, chave) → valor
Retorna o valor associado à chave, ou `0` se não encontrada.
```vit
let v: i32 = map_get(m, 42);
```

#### map_has(m, chave) → bool
Retorna `true` se a chave existe.
```vit
if map_has(m, 42) {
  print map_get(m, 42);
}
```

### Exemplos

#### Two Sum (O(n))
```vit
fn main() -> i32 {
  input n: i32;
  let nums: [i32; 10000];
  for i in 0..n {
    input nums[i];
  }
  input target: i32;

  let m: map[i32, i32];
  for i in 0..n {
    let comp: i32 = target - nums[i];
    if map_has(m, comp) {
      print map_get(m, comp), " ", i;
      return 0;
    }
    map_set(m, nums[i], i);
  }
  return 0;
}
```

#### Frequência de elementos
```vit
fn main() -> i32 {
  input n: i32;
  let m: map[i32, i32];
  for i in 0..n {
    input x: i32;
    map_set(m, x, map_get(m, x) + 1);
  }
  return 0;
}
```

#### Soma acumulada com i64
```vit
let m: map[i32, i64];
map_set(m, k, map_get(m, k) + val as i64);
```

### Tipos suportados

| Tipo do map      | Chave | Valor |
|------------------|-------|-------|
| `map[i32, i32]`  | i32   | i32   |
| `map[i32, i64]`  | i32   | i64   |
| `map[i64, i32]`  | i64   | i32   |
| `map[i64, i64]`  | i64   | i64   |
| `map[str, i32]`  | str   | i32   |
| `map[str, str]`  | str   | str   |

### Limitações do map

| Feature                        | Status                           |
|--------------------------------|----------------------------------|
| Capacidade máxima              | 4096 entradas únicas             |
| Iteração sobre chaves          | Não suportado                    |
| Remoção de entradas            | Não suportado                    |
| map como parâmetro de função   | Suportado                        |
| map global                     | Suportado                        |

---

## Módulos e imports

Vit suporta divisão de código em múltiplos arquivos via `import` e declaração de dependências de linker via `link`. Ambas são diretivas de pré-processamento — resolvidas antes da tokenização.

### import

```vit
import "caminho/relativo/arquivo.vit";
```

O caminho é relativo ao arquivo que contém o `import`. Importações circulares e duplicadas são ignoradas automaticamente.

```vit
// lib/math.vit
fn quadrado(x: i32) -> i32 {
    return x * x;
}
```

```vit
// main.vit
import "lib/math.vit";

fn main() -> i32 {
    print quadrado(7);   // 49
    return 0;
}
```

### link

Declara uma flag de linker necessária para o módulo. O compilador passa a flag automaticamente ao clang — o usuário não precisa especificá-la no CLI.

```vit
link "-lsqlite3";     // linka com libsqlite3
link "-lm";           // linka com libm
link "shim.c";        // compila shim.c e linka automaticamente
```

A diretiva `link` é herdada por quem importa o módulo.

Arquivos `.c` em `link` são **compilados automaticamente** pelo compilador antes do link — o caminho é resolvido relativo ao `.vit` que declarou o `link`:

```vit
// lib/sqlite.vit
link "sqlite_shim.c";   // compilado para /tmp/vit_..._sqlite_shim.o automaticamente
link "-lsqlite3";
```

```vit
// app.vit
import "lib/sqlite.vit";   // sqlite_shim.c compilado e -lsqlite3 linkados automaticamente
```

```bash
vit run app.vit   # sem flags manuais
```

> `import` e `link` devem aparecer no topo do arquivo, antes de qualquer declaração.

---

## Funções externas (extern fn)

Permite chamar qualquer função C sem modificar o compilador. Declara a assinatura da função para que o LLVM gere a chamada corretamente.

### Sintaxe

```vit
extern fn nome_da_funcao(param1: tipo1, param2: tipo2) -> tipo_retorno;
```

Use `void` como tipo de retorno quando a função C não retorna valor.

### Exemplos

```vit
// Funções da libc
extern fn strlen(s: str) -> i64;
extern fn malloc(size: i64) -> str;
extern fn printf(fmt: str) -> i32;
extern fn exit(code: i32) -> void;

fn main() -> i32 {
    let s: str = "hello";
    let n: i64 = strlen(s);
    print n;    // 5
    return 0;
}
```

### Uso com bibliotecas C externas

Declare as funções com `extern fn` e adicione a flag de link com a diretiva `link`. O mais comum é criar um arquivo de módulo em `lib/` que encapsula tudo:

```vit
// lib/sqlite.vit
link "-lsqlite3";

extern fn sqlite3_open(path: str, db: str) -> i32;
extern fn sqlite3_exec(db: str, sql: str, cb: str, arg: str, err: str) -> i32;
extern fn sqlite3_close(db: str) -> i32;

fn db_open(path: str) -> str { ... }
fn db_exec(db: str, sql: str) -> i32 { ... }
```

```vit
// app.vit
import "lib/sqlite.vit";   // -lsqlite3 aplicado automaticamente
```

> Qualquer função C linkável é acessível via `extern fn` — não é necessário alterar o compilador.

---

## Stdlib

Módulos instalados em `~/.vit/lib/` pelo script de instalação. Importados por caminho — o compilador procura localmente primeiro, depois em `~/.vit/lib/`.

### lib/net.vit — TCP/Sockets

```vit
import "lib/net.vit";
```

| Função | Assinatura | Descrição |
|--------|-----------|-----------|
| `tcp_listen` | `(port: i32) -> i32` | Abre servidor TCP na porta, retorna fd |
| `tcp_accept` | `(server_fd: i32) -> i32` | Aceita conexão, retorna fd do cliente |
| `tcp_read`   | `(fd: i32, buf: str, size: i32) -> i32` | Lê dados do cliente |
| `tcp_write`  | `(fd: i32, data: str, len: i32) -> i32` | Envia dados |
| `tcp_close`  | `(fd: i32) -> i32` | Fecha conexão |

### lib/http.vit — Servidor HTTP

```vit
import "lib/http.vit";
```

Depende de `lib/net.vit` (importado automaticamente).

#### Tipos

```vit
struct Request {
    method:  str,
    path:    str,
    body:    str,
    headers: map[str, str],
}
```

#### Parsing e roteamento

| Função | Descrição |
|--------|-----------|
| `http_parse(buf)` | Parseia raw HTTP/1.x, retorna `Request` |
| `http_is(req, method, path)` | Match exato de método + path (ignora query string) |
| `http_starts_with(req, method, prefix)` | Match de prefixo (rotas dinâmicas) |
| `http_path_clean(req)` | Path sem query string |
| `http_header(req, name)` | Valor de header ou `""` |
| `http_method(req)` | Método HTTP |
| `http_path(req)` | Path completo |
| `http_body(req)` | Body da request |

#### Form e query string

| Função | Descrição |
|--------|-----------|
| `form_get(body, key)` | Extrai valor de body form-urlencoded |
| `form_has(body, key)` | `1` se a chave existe no body |
| `query_get(req, key)` | Extrai valor da query string (`?key=val`) |
| `query_has(req, key)` | `1` se a chave existe na query string |
| `query_str(req)` | Query string bruta |

#### Respostas

| Função | Status |
|--------|--------|
| `http_ok(body)` | 200 text/plain |
| `http_json(body)` | 200 application/json |
| `http_created(body)` | 201 text/plain |
| `http_json_created(body)` | 201 application/json |
| `http_no_content()` | 204 |
| `http_bad_request(msg)` | 400 |
| `http_unauthorized(msg)` | 401 |
| `http_forbidden(msg)` | 403 |
| `http_not_found()` | 404 |
| `http_unprocessable(msg)` | 422 |
| `http_error(msg)` | 500 |

#### Servidor

| Função | Descrição |
|--------|-----------|
| `http_handle(method, path, fn)` | Registra handler |
| `http_listen(port)` | Inicia loop de atendimento |

```vit
import "lib/http.vit";

fn handle_hello(req: Request) -> str {
    let name: str = query_get(req, "name");
    return http_json(format("{\"hello\":\"%s\"}", name));
}

fn main() -> i32 {
    http_handle("GET", "/hello", handle_hello);
    http_listen(8080);
    return 0;
}
```

### lib/json.vit — JSON builder

```vit
import "lib/json.vit";
```

Zero dependências externas — construído sobre `StrBuf`.

#### Objeto

| Função | Descrição |
|--------|-----------|
| `json_new()` | Novo objeto builder |
| `json_str(j, key, val)` | Adiciona `"key": "val"` |
| `json_int(j, key, val)` | Adiciona `"key": 123` |
| `json_bool(j, key, val)` | Adiciona `"key": true/false` |
| `json_null(j, key)` | Adiciona `"key": null` |
| `json_raw(j, key, val)` | Adiciona `"key": <val bruto>` (para aninhar) |
| `json_build(j)` | Retorna `{...}` |

#### Array

| Função | Descrição |
|--------|-----------|
| `json_arr_new()` | Novo array builder |
| `json_arr_str(a, val)` | Appenda `"val"` |
| `json_arr_int(a, val)` | Appenda `123` |
| `json_arr_obj(a, val)` | Appenda objeto pré-construído |
| `json_arr_build(a)` | Retorna `[...]` |

```vit
import "lib/json.vit";

fn main() -> i32 {
    let j: StrBuf = json_new();
    json_bool(j, "ok", 1);
    json_str(j, "msg", "hello");
    print json_build(j);   // {"ok":true,"msg":"hello"}
    return 0;
}
```

### lib/sqlite.vit — SQLite3

```vit
import "lib/sqlite.vit";
```

**Dependência:** `sudo apt install libsqlite3-dev`

O shim C é compilado automaticamente via `link "sqlite_shim.c"`.

| Função | Descrição |
|--------|-----------|
| `sqlite_open(filename)` | Abre/cria banco, retorna handle |
| `sqlite_close(db)` | Fecha conexão |
| `sqlite_exec(db, sql)` | Executa SQL sem parâmetros |
| `sqlite_prepare(db, sql)` | Compila statement (`?1`, `?2`, ...) |
| `sqlite_bind(stmt, idx, val)` | Vincula parâmetro texto (1-based) |
| `sqlite_step(stmt)` | Avança linha — retorna `SQLITE_ROW` ou `SQLITE_DONE` |
| `sqlite_col_text(stmt, col)` | Valor texto da coluna (0-based) |
| `sqlite_col_int(stmt, col)` | Valor inteiro da coluna |
| `sqlite_finalize(stmt)` | Libera statement |
| `sqlite_errmsg(db)` | Última mensagem de erro |

Constantes: `SQLITE_OK = 0`, `SQLITE_ROW = 100`, `SQLITE_DONE = 101`

```vit
import "lib/sqlite.vit";

let db: str;

fn main() -> i32 {
    db = sqlite_open("app.db");
    sqlite_exec(db, "CREATE TABLE IF NOT EXISTS items (name TEXT)");

    let stmt: str = sqlite_prepare(db, "INSERT INTO items VALUES (?1)");
    sqlite_bind(stmt, 1, "foo");
    sqlite_step(stmt);
    sqlite_finalize(stmt);

    return 0;
}
```

### lib/env.vit — Variáveis de ambiente

```vit
import "lib/env.vit";
```

| Função | Descrição |
|--------|-----------|
| `env_get(name)` | Valor da variável ou `""` se não definida |
| `env_or(name, default)` | Valor da variável ou `default` |
| `env_has(name)` | `1` se definida e não-vazia |

```vit
import "lib/env.vit";

fn main() -> i32 {
    let port: str = env_or("PORT", "8080");
    let db:   str = env_or("DATABASE_URL", "app.db");
    print port;
    return 0;
}
```

```bash
PORT=9090 DATABASE_URL=/data/prod.db ./meu_app
```

---

## Limitações conhecidas

| Feature                                 | Status                            |
|-----------------------------------------|-----------------------------------|
| `str` como parâmetro/retorno            | Suportado                         |
| Arrays como parâmetros                  | Suportado (por ponteiro)          |
| Structs aninhadas                       | Suportado                         |
| `map` como parâmetro de função          | Suportado                         |
| `map` global                            | Suportado                         |
| Módulos / imports                       | Suportado                         |
| Diretiva `link` (flags e arquivos `.c`) | Suportado                         |
| `extern fn` (FFI com C)                 | Suportado                         |
| `format(fmt, ...) -> str`               | Suportado                         |
| Arrays multidimensionais                | Não suportado                     |
| Array como campo de struct              | Não suportado                     |
| Iteração sobre chaves de map            | Não suportado                     |
| `for` com passo ≠ 1                     | Não suportado                     |
| Conversão implícita de tipos            | Não suportada (use `as`)          |
| `f32` com literal float                 | Não suportado (use `f64`)         |
| Checagem de bounds em arrays            | Não implementada                  |
| Inferência de tipo (`let x = 42`)       | Não suportada                     |
| Inicializadores de globais não-literais | Não suportado                     |
| sort descendente                        | Não suportado diretamente         |
| Alocação dinâmica (Vec)                 | Não suportada                     |
| Generics / templates                    | Não suportado                     |

---

## Compilação e execução

### Instalação

```bash
# Clona o repositório e executa o script de instalação:
git clone https://github.com/joaofaugusto/vit
cd vit
bash install.sh
```

O script instala o compilador via `cargo install` e copia a stdlib para `~/.vit/lib/`.

Após a instalação, `vit` está disponível no PATH e a stdlib é resolvida automaticamente — nenhum argumento extra necessário.

### Comandos

```bash
vit build <arquivo.vit>              # compila, gera binário no diretório atual
vit build <arquivo.vit> <output>     # especifica nome do binário
vit run   <arquivo.vit>              # compila e executa em seguida
```

```bash
# Exemplos
vit build server.vit
vit run   hello.vit
vit build app.vit meu_app
```

### Flags de debug

```bash
vit build arquivo.vit --verbose   # exibe tokens, AST e LLVM IR
vit run   arquivo.vit -v          # forma curta
```

### Flags de linker extras (CLI)

Normalmente desnecessário — libs declaram suas próprias dependências com `link`. Mas é possível passar flags extras:

```bash
vit build app.vit -lm             # linka com libm
vit build app.vit extra.c         # inclui arquivo C extra
vit build app.vit myapp -lcurl    # output nomeado + flag
```

### Arquivos gerados

| Arquivo | Local | Descrição |
|---------|-------|-----------|
| `nome.ll` | `/tmp/` | LLVM IR (debug) |
| `nome.o`  | `/tmp/` | Objeto ELF |
| `nome`    | diretório atual | Executável final |

Os intermediários vão para `/tmp/` — o diretório do projeto fica limpo.

### Dependências do sistema

```bash
# Ubuntu/Debian (WSL incluso)
sudo apt install llvm-18 clang libsqlite3-dev
```
