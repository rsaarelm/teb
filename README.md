# TEB - Table Evaluator

Formats whitespace-separated plaintext tables into aligned columns and evaluates simple spreadsheet formulas embedded in them.

Usage: Compile the Rust program and install it on your path.
Write unformatted tables and pipe them through `teb` from your editor to get the columns formatted and the formulas evaluated.

Spreadsheet formulas are preceded by an ASCII left angle bracket `<` (to indicate the value coming from the right) and use stack-based tacit programming with the preceding row values used as stack values.
The syntax for formulas is influenced by [Uiua](https://www.uiua.org/).

Teb uses the same whitespace-separated table style as [IDM](https://github.com/rsaarelm/idm) and can be used to format IDM tables.

## Examples

### Shopping list

Input:

```
item unit qty cost
milk 3 2 <×
bananas 0.5 6 <
flour 1.2 1 <
- - total: <⇓/+
```

The input is piped through `teb` to yield formatted output with formula results calculated before the angle brackets.

Formatted output:

```
item     unit  qty     cost
milk     3     2        6<×
bananas  0.5   6        3<
flour    1.2   1        1.2<
-        -     total:  10.2<⇓/+
```

Formulas are written after the angle brackets.
Formulas use reverse Polish notations and take values on the row they're on as stack arguments.
An empty left angle bracket will reuse the last formula seen on the same column.

You can operate on the column above a formula by pulling it in as an array value using the `⇓` operation.
The reduce modifier `/` recursively applies its operand to all array values, so `/+` produces a sum of the array's values.

### Account running total

Add transaction to the previous balance above on each row.
∘ is the identity function that just repeats the last value.

```
transaction     amount  balance
deposit         1000    1000<∘
train_ticket     -20     980<⇓⊣+
utility_fee     -120     860<
scratch_ticket   200    1060<
gas              -60    1000<
```

In this case we only want the last value of the array so we use the last (`⊣`) operator on it.

### Body-mass-index calculation

The rearrange operator `.` takes a list of subscripts of stack indices to move to the top of the stack.
There's a convenience operator `²` for squaring a value. We use `.₂` to bring height (the second stack value) to the top to be squared before weight is divided by it to get the BMI.
You get the average of the column by using the fork (`⊃`) modifier that applies two operations to input.
`#` returns the number of rows in the array.

```
--        height(m)  weight(kg)  BMI
Alice     1.62        56         21.34<.₂²÷
Bob       1.70        68         23.53<
Carol     1.78        74         23.36<
Vladimir  1.92       230         62.39<
-         -          Avg:        32.65<⇓⊃/+#÷
```

### Antimatter bomb yields

You can assign constants in a separate table above the main one and they'll persist.
Use scientific notation in the input cells and output will also use it.

Text blocks separated by an empty line are treated as separate tables in terms of formatting and column operations.
Assigned variables will persist to lower tables when running `teb` over multiple tables though.

You can add freeform comment text after the last column shared by every row, this text will not be formatted or evaluated.
In the main table, whitespace is a column separator, so if you want a column to contain text phrases you need to use underscores in place of spaces.

```
--      -
2.99e8  <→c  speed of light

--          mass(kg)  yield(J)
Little_Boy  4.4e3        6e13      Historical bombs for reference
Tsar_Bomba   27e3        2e17
-           -         -
Mark_1        2e0     1.79e17<c²×
Mark_2      4.5e0     4.02e17<c²×
```

### Grade averages

To compute over the whole row, use `]` to collapse the stack elements into a single array value.

```
--        math  physics  cs  literature  avg
Alice     92    74       83  34          70.75<]⊃/+#÷
Bob       84    69       89  48          72.5<
Carol     68    94       75  79          79<
Vladimir  45    52       92  95          71<
```
