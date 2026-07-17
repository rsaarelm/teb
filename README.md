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
With no parameters, the rearrange operator doubles the top argument and it can be used to display the input cell with no modifications.
You get the average of the column by using the fork (`⊃`) modifier that applies two operations to input.
`#` returns the number of rows in the array.

```
---       height(m)  weight(kg)  BMI
Alice     1.62        56         21.34<.₂²÷
Bob       1.70        68         23.53<
Carol     1.78        74         23.36<
Vladimir  1.92       230         62.39<
-         -          Avg:        32.65<⇓⊃/+#÷
```

### Antimatter bomb yields

You can assign constants in a separate table above the main one and they'll persist.

Text blocks separated by an empty line are treated as separate tables in terms of formatting and column operations.
Assigned variables will persist to lower tables when running `teb` over multiple tables though.

You can use special formats by typing cells according to recognized special syntaxes.
You can tell an output cell to output using a given syntax by writing it a dummy value in the format you want.
If you use large numbers, you want to use scientific notation.
Write `0e0` or just `e` in your output cell to request that the output is printed in scientific notation.
Cells with just a `<` will inherit both the formula and the formatting from the output cell above them.

You can add freeform comment text after the last column shared by every row, this text will not be formatted or evaluated.
In the main table, whitespace is a column separator, so if you want a column to contain text phrases you need to use underscores in place of spaces.

Input, note the `e` in the output field for Mark 1 bomb:

```
- -
2.99e8 <→c speed of light

--- mass(kg) yield(J)
Little_Boy 4400 6e13 Historical bombs for reference
Tsar_Bomba 27000 2e17
- - -
Mark_1 2 e<c²×
Mark_2 4.5 <
```

Output:

```
-       -
2.99e8  <→c  speed of light

---         mass(kg)  yield(J)
Little_Boy   4400        6e13      Historical bombs for reference
Tsar_Bomba  27000        2e17
-           -         -
Mark_1          2     1.79e17<c²×
Mark_2          4.5   4.02e17<
```

### Project plan

A project plan will have a start date and tasks with estimated durations, the spreadsheet will calculate the estimated finish day.
For this we need dates, formatted like `1970-12-31` and day durations, formatted with a 'd' suffix like `12d`
Time and date formatted values are all treated internally as seconds.
Date values will turn into seconds from the Unix epoch.

The output cells start with dummy values.

```
start_date 2026-01-20 <→a

task est. total completed
req_gather 12d d<. 1970-01-01<a+
design 15d d<⇓⊣+ <
assembly 24d < <
calibration 4d < <
final_check 2d < <
```

Running this through `teb` we get:

```
start_date  2026-01-20  <→a

task         est.  total    completed
req_gather   12d   12d<.    2026-02-01<a+
design       15d   27d<⇓⊣+  2026-02-16<
assembly     24d   51d<     2026-03-12<
calibration   4d   55d<     2026-03-16<
final_check   2d   57d<     2026-03-18<
```

The example doesn't account for people not working on weekends, but you can just multiple all your time estimates by 1.5 when entering them to correct for this.

### Grade averages

To compute over the whole row, use `]` to collapse the stack elements into a single array value.

```
---       math  physics  cs  literature  avg
Alice     92    74       83  34          70.75<]⊃/+#÷
Bob       84    69       89  48          72.5<
Carol     68    94       75  79          79<
Vladimir  45    52       92  95          71<
```

### Mean time between failures

We have a system log of times of failure as RFC 3339 timestamps, and we want to find out the mean time between failures.
Timestamps are one accepted input format, and they resolve as the corresponding Unix time.

The command to pull the column above can be given a subscript to pull a column from further to the left.
To get the difference between the last two error times, we get the top from the above column to the left (which stops one cell before the current row, just like the column above the input cell, and then subtract it from the current timestamp to the left.
Format the differences between timestamps to days to keep them readable.
Then we just use the old mean formula to count the mean interval.

log                   interval
1990-04-01T10:35:22Z  -
1990-05-04T13:35:30Z  33d<⇓₁⊣-
1990-08-05T23:07:10Z  93d<
1990-10-29T14:28:37Z  85d<
1990-12-29T05:01:01Z  61d<
1991-02-28T10:45:51Z  61d<
1991-03-23T10:00:28Z  23d<
1991-03-31T07:26:44Z   8d<
1991-06-06T00:06:37Z  67d<
1991-06-09T17:25:46Z   4d<
mtbf:                 48d<⇓⊃/+#÷
