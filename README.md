# NEO Language -- A programming language for the NEO platform

`neo-lang` is a contract-oriented programming language to write smart contracts for the NEO blockchain.
It is a high-level programming language that is designed to be easy to use.

## Quick Start

```
// Define a contract. Any `neo-lang` program must be a(only one) contract.
#[auther("AuthorName")]
#[version("0.0.1")]
contract Example {
    // Declare a constant string property
    const string symbol = "Example";

    // Declare a constant int property
    const int decimals = 8;

    // Declare a int property.
    int totalSupply = 1000;

    // Declare a map property.
    map[hash160, int] balances;

    // Declare a method
    bool transfer(hash160 source, hash160 dest, int amount) {
        assert(amount > 0, "Amount must be greater than 0");
        // ...
        return true; // or return false;
    }
}

```

## Built-in Types
`neo-lang` is strongly typed language, and the built-in types are:
- `null`: The null value. Literal: `null`.
- `bool`: The boolean type. Literal: `true` or `false`.
- `int`: The integer type. (256-bit signed integer, litteral: prefix: Hex(`0x`, `0X`), Binary(`0b`, `0B`), Decimal(no prefix)).
- `string`: The byte-string type. (immutable string in bytes. NOTE: no Unicode support). Literal: `"string"`.
- `hash160`: The 160-bit hash type, and the underlying type is 20-byte byte-string(i.e. string),  Literal: `"0x1234567890abcdef1234567890abcdef12345678"`.
- `hash256`: The 256-bit hash type, and the underlying type is 32-byte byte-string(i.e. string), Literal: `"0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef"`.
- `type[]`: The array type. (dynamic array for generic type). Literal: `int[] { 1, 2, 3 }` (element type required; use `{`…`}` for elements, not `[`…`]`).
- `map[key, value]`: The map type. key-type is limited to `bool`, `int`, `string`, `hash160`, `hash256`, and value-type can be any type. Literal: `map[string, string] { "key1": "value1", "key2": "value2" }`, or `map[int, string] { 1: "value1", 2: "value2" }`.
- `buffer`: The buffer type. (dynamic bytes buffer). Literal: `b"1234567890"`.
- `any`: The any type. Literal: any of the above types.

## Self-defined Types

A self-defined type, i.e a struct, is declared with the `struct` keyword, then the type name and the fields.
The fields are the data to be stored in the type.

Example:
```
struct Transfer {
    // The field is not initialized, so it will be initialized with the default value of the type.
    hash160 source;

    hash160 dest;

    // The field is initialized with the default value of the type.
    // The initialization value must be literal.
    int amount = 0;
}

// Initialize a struct instance.
var transfer = Transfer {
    source: "0x1234567890abcdef1234567890abcdef12345678",
    dest: "0x1234567890abcdef1234567890abcdef12345678",
    amount: 100,
};
```

A struct body may also contain **methods** (same shape as a small function: return type, name, parameters, block). The implicit receiver is `self`; the compiler lowers each method to a function named `StructName::methodName` with a leading `self` parameter of the struct type. Instance calls like `point.distanceTo(other)` are not implemented yet (they would require VM `CALL` linking); use package-level helpers or wait for that backend work.

### Type Conversion
Type conversion is used to convert one type to another type.

Example:
```
var a = 1;           // a is int
var b = a as bool;   // b is true
var s = b as string; // s is "true"
```

- `int` to `bool`: `0` is `false`, other values are `true`
- `bool` to `int`: `false` is `0`, `true` is `1`
- `int` to `string`: in base 10 and without any prefix or suffix
- `string` to `int`: in base 10 and without any prefix or suffix
- `string` to `bool`: empty string is `false`, other values are `true`
- `bool` to `string`: `false` is `"false"`, `true` is `"true"`
- Other type conversions are not invalid.

## Basic Operations
### Arithmetic/Bitwise Operations on `int` Type
- `+`: Addition, or unary plus
- `-`: Subtraction, or unary minus
- `*`: Multiplication
- `/`: Division
- `%`: Modulus
- `>>`: Right shift(preserving sign)
- `<<`: Left shift(zero fill)
- `&`: Bitwise AND
- `|`: Bitwise OR
- `^`: Bitwise XOR
- `~`: Bitwise NOT
- `+=`: Add and assign
- `-=`: Subtract and assign
- `*=`: Multiply and assign
- `/=`: Divide and assign
- `%=`: Modulus and assign
- `>>=`: Right shift(preserving sign) and assign
- `<<=`: Left shift(zero fill) and assign
- `&=`: Bitwise AND and assign
- `|=`: Bitwise OR and assign
- `^=`: Bitwise XOR and assign

NOTE: any underflow, overflow, division by zero, will cause execution failure.

### Comparison Operations
- `==`: Equal: checks value equality for bool, int, string, hash160, hash256, and checks if the items are the same or not for array, buffer, map.
- `!=`: Not equal, the opposite of `==`.
- `>`: Greater than, for int, string, hash160, hash256, and invalid for other types.
- `>=`: Greater than or equal to, for int, string, hash160, hash256, and invalid for other types.
- `<`: Less than, for int, string, hash160, hash256, and invalid for other types.
- `<=`: Less than or equal to, for int, string, hash160, hash256, and invalid for other types.


### Logical Operations
- `!`: Logical NOT.
- `&&`: Logical AND. Short-circuit evaluation.
- `||`: Logical OR. Short-circuit evaluation.

### Operator precedence
- 1. unary operations: `-`(unary minus), `+`(unary plus), `!`(logical NOT). From left to right.
- 2. `as` type conversion. From left to right.
- 3. `*`, `/`, `%`. From left to right.
- 4. `+`(addition), `-`(subtraction). From left to right.
- 5. `<<`, `>>`. From left to right.
- 6. `&`. From left to right.
- 7. `|`. From left to right.
- 8. `^`. From left to right.
- 9. `~`. From left to right.
- 10. `==`, `!=`, `<`, `<=`, `>`, `>=`. From left to right.
- 11. `&&`. From left to right.
- 12. `||`. From left to right.
- 13. `=`, `+=`, `-=`, `*=`, `/=`, `%=`, `>>=`, `<<=`, `&=`, `|=`, `^=`. From right to left.

1 is the highest precedence, and 13 is the lowest precedence.
Is's better to use parentheses to group the expressions to avoid confusion.


## Control Flow
### If Statement
```
// The condition must be an expression that evaluates to a bool value.
if condition {
    // code to execute if condition is true
} else {
    // code to execute if condition is false(optional)
}
```

### For Loop

For loop for `array`:
```
for item in array {
    // code to execute for each item
}
```

For loop for `map`:
```
for key, value in map {
    // code to execute for each item
}
```

### While Loop
```
// The condition must be an expression that evaluates to a bool value.
while condition {
    // code to execute while condition is true
}
```


## Contract
`neo-lang` provides `contract` keyword to define a contract. A contract contains a set of properties, events and functions.
```
contract Example {
    
    // Declare a constant property
    const string symbol = "Example";

    // Declare a constant property
    const int decimals = 8;

    // Declare a int property.
    int totalSupply = 10000;
    
    // Declare a string property.
    string owner;

    // Declare a map property.
    map[hash160, int] balances;

    // Declare a event
    event transfer(hash160 source, hash160 dest, int amount);

    // Declare a method
    bool transfer(hash160 source, hash160 dest, int amount) {
        assert(amount > 0, "Amount must be greater than 0");

        // Cannot load whole map, must use `[key]` to load a single entry.
        var balance = self.balances[source]; // Avoid multiple load operations
        assert(balance >= amount, "Insufficient balance");
        self.balances[source] = balance - amount;
        self.balances[dest] = self.balances[dest] + amount;
        emit transfer(source, dest, amount);
        return true; // or return false if the transfer is not successful
    }

    int balanceOf(hash160 account) {
        return self.balances[account];
    }
}
```

### Properties
Properties are used to store data in a contract. There are two types of properties: constant properties and mutable properties.

#### Constant Properties
- A constant property is declared with `const` keyword, then the type, name of the property and it must be initialized when declared.
- Constant property is not stored in the contract storage, it is part of the contract code.
- Only `bool`, `int`, `string`, `hash160`, `hash256` types are allowed to be used as constant properties.

Example:
```
const string symbol = "Example";
const int decimals = 8;
const hash160 owner = "0x1234567890abcdef1234567890abcdef12345678";
```

#### Mutable Properties
Mutable properties are used to store data to the contract storage, and can be changed during the contract execution.

A mutable property is declared with the type, name of the property.
The initialization is optional, and if not initialized, the property will be initialized with the default value of the type.
The default value of the type is:
- `bool`: false
- `int`: 0
- `string`: empty string
- `hash160`: all zeros
- `hash256`: all zeros
- `map`: empty map

NOTE: `contract` cannot have array properties, and only partial map methods are supported.

Example:
```
int totalSupply = 10000;
map[hash160, int] balances;
```

Mutable property read and write will cause contract storage load/store operations, and these operations are expensive.
Therefore, it is recommended to declare as many constant properties as possible to reduce the contract storage usage or
avoid load/store multiple times if data are not changed.

For example:
```
bool mint(int amount) {
    assert(caller.sender == owner, "Only owner can mint");
    assert(amount > 0, "Amount must be greater than 0");

    var total = self.totalSupply;
    if total > 1000 {
        return false;
    }

    self.totalSupply = total + amount;
    return true;
}
```

### Events
Events are used to notify the contract events to the caller.

An event is declared with the `event` keyword, then the event name and the parameters.
The parameters are the data to be emitted when the event is triggered.

A event is triggered by the `emit` keyword, then the event name and the parameters.

Example:
```
event transfer(hash160 source, hash160 dest, int amount);

void _onTransfer(hash160 source, hash160 dest, int amount)  {
    emit transfer(source, dest, amount);
}
```

### Function/Method
Function/Methods are used to define the contract behavior.

A function/method is declared with the return type, function/method name and the parameters.

Example:
```
bool transfer(hash160 source, hash160 dest, int amount) {
    assert(amount > 0, "Amount must be greater than 0");
    var balance = balances[source]; // Avoid multiple load operations
    assert(balance >= amount, "Insufficient balance");
    balances[source] = balance - amount;
    balances[dest] += amount;
    emit transfer(source, dest, amount);
    return true;
}
```

A function/method name starting with `_` is a private function/method.
Private function/methods are used to encapsulate the internal logic of the contract, and are not visible to the caller.
For contract public properties, the compiler will generate a getter function for the property, and transaction and other contracts can call the getter function to get the property value.

## Attributes
Attributes are used to add metadata to the contract, function, method, event, property.

An attribute is declared with the  `#[attribute]`, `#[attribute(value)]` syntax, then the attribute name and the value(or value list).
The value(or value list) is optional.

Example:
```
#[auther("AuthorName")]
#[version("0.0.1")]
contract Example {
    // ...
}
```

### Built-in Attributes
- `#[auther("AuthorName")]`: For contract, the author.
- `#[version("0.0.1")]`: For contract, the version.
- `#[pure]`: For contract method, mark the method as pure(never store data, never emit events, never call other contracts).
- `#[noreentrant]`: For contract method, mark the method as non-reentrant.


### Package

Library is a or multiple packages, and a package is a collection of functions/structs that can be used by other contracts.
Library cannot be compiled independently, it must be imported by contract.
A package is declared with the `package` keyword, then the package name and the functions/structs.

Example:
```
// Declare a package. It must be the first statement in the source file.
package math;

// A package can contain multiple functions.
int unsignedAdd(int a, int b) {
    assert(a > 0, "a must be greater than 0");
    assert(b > 0, "b must be greater than 0");
    return a + b;
}

// A package can contain multiple structs.
// A struct contains fields, and methods.
// Unlike contract, struct fileds are saved in memory.
struct Point {
    // `_x` and `_y` starts with `_`, so they are private fields.
    int _x;
    int _y;
}

```

## Runtime
The `runtime` is a global package that provides the runtime information of the current contract and blockchain.
It contains the following functions:
  - `int currentBlockHeight()`: the height of the current generating block;
  - `int network()`: the network id of the current blockchain;
  - `int random()`: returns a random int(deterministic);
  - `any[] call(hash160 contract, string method, any[] params)`: calls a method of a contract.
  - `void log(string message)`: logs a message.
  - `void notify(string event, any[] params)`: notifies an event to the current context.
  - ..., more methods.


## Built-in Functions
- `void assert(bool condition, string message)`: assert the condition is true, otherwise throw an exception with the message.
- `void abort(string message)`: abort the current execution with the message.
- `int min(int a, int b)`: return the minimum of two integers.
- `int max(int a, int b)`: return the maximum of two integers.

## Built-in Methods
- `string`:
  - `int string.size()`: return the bytes length;
  - `string string.sub(int start, int length)`: return the sub-string by the start and length.

- `buffer`:
  - `int buffer.size()`: return the bytes length;
  - `buffer buffer.sub(int start, int length)`: return the sub-buffer by the start and length;

- `int`:
  - `int int.sqrt()`: return the square root;
  - `int int.modmul(int other, int modulus)`: return the modulus multiplication of the other and the modulus;
  - `int int.modpow(int exponent, int modulus)`: return the modulus power of the exponent and the modulus;
  - `int int.within(int minInclusive, int maxExclusive)`: return the value within the range `[minInclusive, maxExclusive)`;

- `array`:
  - `int array.size()`: return the length;
  - `void array.push(type value)`: push the value to the array;
  - `type array.pop()`: pop the last value from the array;
  - `void array.clear()`: clear the array;

- `map`:
  - `int map.size()`: return the length;
  - `type[] map.keys()`: return the keys as an array;
  - `type[] map.values()`: return the values as an array;
  - `bool map.has(key)`: return true if the key exists in the map;
  - `void map.clear()`: clear the map;
  - `void map.remove(key)`: remove the key from the map;
  - contract persistent property map only supports `has`, `remove`, and index access methods.

## Native Contracts
- `ContractManagement`: 
- `StdLib`:
- `CryptoLib`:
- `Ledger`:
- `NEO`:
- `GAS`:
- `Policy`:
- `RoleManagement`:
- `Oracle`:
- `Notary`:

## Call Other Contracts
There are two ways to call other contracts:
The first way is to use `ContractName.method` to call a method of another contract.
Example:
```
var result = ContractName.method(arg0, arg1, ...);
```
The called contract must be deployed on the same network.
This way will not create a new ExecutionContext.


The second way is to use `runtime.call` to call a method of a contract.
Example:
```
var result = runtime.call(contract, "method", args);
```
The called contract must be deployed on the same network.
This way will create a new ExecutionContext.

## Import Package
Use `import` keyword to import package from library, and the used package part will be compiled into the current contract.


Import a package will add the functions/structs of the package to use them in the current contract.
Example:
```
import packagename from "library";

struct StructExample {
    packagename.Struct field;
}

void callExample() {
    packagename.function(params);
}
```

## Keywords
`contract`, `package`, `struct`, `import`, `const`, `event`, `emit`, `return`, `if`, `else`,
`for`, `in`, `while`, `var`, `as`, `void`, `bool`, `int`, `string`, `hash160`,
`hash256`, `map`, `buffer`, `any`, `null`, `true`, `false`, `self`.

## Build-in Identifiers
`runtime`, `assert`, `abort`, `min`, `max`.

## Code style
- Use 4 spaces for indentation.
- Use camelCase for variable, event, function names.
- Use PascalCase for struct, contract names.
- Use lowercase for package names.
- Use lowercase and "_" for file name, and the file name suffix must be `.neo`.
