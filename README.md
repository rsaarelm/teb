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
The arithmetic operators are `+`, `-`, `×` and `÷`.
Unicode symbols are used unapologetically, since it's imperative to get the formulas as compact as possible to fit them in the table layout without too much disruption.
An empty left angle bracket will reuse the last formula seen on the same column.

You can operate on the column above a formula by pulling it in as an array value using the `⇓` operation.
The reduce modifier `/` recursively applies its operand to all array values, so `/+` produces a sum of the array's values.

### Account running total

Add transaction to the previous balance above on each row.
∘ is the identity function that just repeats the last value.
You need to have something in a formula even if you just want to repeat the top input stack value, since the default stack values built from the table row before the formula starts executing never qualify as cell output.

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

### Grade averages

To compute over the whole row, use `]` to collapse the stack elements into a single array value.

```
---       math  physics  cs  literature  avg
Alice     92    74       83  34          70.75<]⊃/+#÷
Bob       84    69       89  48          72.5<
Carol     68    94       75  79          79<
Vladimir  45    52       92  95          71<
```

### Antimatter bomb yields

You can assign constants in a separate table above the main one and they'll persist.

Text blocks separated by an empty line are treated as separate tables in terms of formatting and column operations.
Assigned variables will persist to lower tables when running `teb` over multiple tables though.

You can use special formats by typing cells according to recognized special syntaxes.
You can tell an output cell to output using a given syntax by writing it a dummy value in the format you want.
If you use large numbers, you want to use scientific notation.
Write `0e0` or just `e` as the value in your output cell to request that the output is printed in scientific notation.
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

### Project estimation

A project plan will have a start date and tasks with estimated durations, the spreadsheet will calculate the estimated finish day.
For this we need dates, formatted like `1970-12-31` and day durations, formatted with a 'd' suffix like `12d`
Time and date formatted values are all treated internally as seconds.
Date values will turn into seconds from the Unix epoch.

Input, the first date output cell has a dummy date to set output column format to dates:

```
task est. completed
start_date - - 2026-01-20
req_gather 12d 1970-01-01<⇓⊣+
design 15d <
assembly 24d <
calibration 4d <
final_check 2d <
```

Running this through `teb` we get:

```
task         est.  completed
start_date   -     2026-01-20
req_gather   12d   2026-02-01<⇓⊣+
design       15d   2026-02-16<
assembly     24d   2026-03-12<
calibration   4d   2026-03-16<
final_check   2d   2026-03-18<
```

The example doesn't account for people not working on weekends, just multiple all your time estimates by 1.5 when entering them to correct for this.

### Mean time between failures

We have a system log of times of failure as RFC 3339 timestamps, and we want to find out the mean time between failures.
Timestamps are one accepted input format, and they resolve as the corresponding Unix time.

The command to pull the column above can be given a subscript to pull a column from further to the left.
To get the difference between the last two error times, we get the top from the above column to the left (which stops one cell before the current row, just like the column above the input cell), and then subtract it from the current timestamp to the left.
Format the differences between timestamps to days to keep them readable.
Then we use the familiar mean formula to count the mean interval.

```
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
```

### Linear RGB to sRGB conversion

Computer monitors use an artificial color space called [sRGB](https://en.wikipedia.org/wiki/SRGB), whose luminance values do not correspond to physical brightness in an obvious way.
We call the physically based color space "linear RGB" to distinguish it from sRGB.
The formula to convert physical RGB luminance *a* to sRGB *s* is

```
s(a) = 12.92a                  if a ≤ 0.0031308
       1.055a^{1/2.4} − 0.055  otherwise
```

We want a grayscale color ramp that has simple fractions of the linear RGB range as its values, because these will produce clean [Bayer dithering](https://en.wikipedia.org/wiki/Ordered_dithering) patterns.
Instead of encoding the conditional in the formula, we just switch formulas in the table once we're past the threshold.
All our values end up using the exponential range anyway.
Operator `⨪` is a reciprocal and is used to turn 2.4 into 1 / 2.4.
Operator `ⁿ` raises the second stack value to the power of the first stack value.

Hex formatting can be required by using `0x0` for the value and binary formatting using `0b0`.
Hex or binary numbers can be assigned padding by having the initial value have heading zeroes.
Initial output formatter `0x0000` will specify four digit padding, so subsequent outputs will look like `0x00a0` rather than `0xa0`.
In this case we want bytes for the sRGB color so we use `0x00` as the initial formatter value.

```
gray  linear     sRGB-float               sRGB-byte
 0    0          0<12.92×                 0x00<255×⌊  Black
-     0.0031307  0.040<                   -
-     0.0031308  0.040<2.4⨪ⁿ1.055×0.055-  0x0a<       threshold for exponential formula
 1    0.062<16÷  0.28<                    0x46<
 2    0.12<      0.39<                    0x63<
 4    0.25<      0.54<                    0x88<
 8    0.5<       0.74<                    0xbb<       Linear middle gray
12    0.75<      0.88<                    0xe0<
16    1<         1.0                      0xff<       White
```

### Bootleg Beeminder

[Beeminder](https://www.beeminder.com/) is a system for tracking your progress on quantifiable goals and alerting you if you are performing too poorly on them.
You can roll your own with Teb, set up the goal as a linear polynomial `y = ax+b` and compare a time series of your recorded tasks to it.

Let's say you set a goal to read at least one book a week.
The parameters of the goal line are the start date and the reciprocal of the interval you want (one week ie. 7 days).

You're reading books, so each entry corresponds to one book read and you can make your accumulation formula just tick up by one each row.

Finally you set up the error column comparing your progress to the goal line.
If the error ever goes negative, you've fallen behind on your goal and should punish yourself as you see fit for this shortcoming.

```
start_date  2026-02-05   <→b
interval             7d  <⨪→a

date        accum   error
-           0       -
2026-02-12  1<⇓⊣1+   0<.₂b-a*-  Catcher in the Rye
2026-02-18  2<       0.14<      The Great Gatsby
2026-02-22  3<       0.57<      The Brothers Karamazov
2026-02-28  4<       0.71<      Blood Meridian
2026-03-10  5<       0.29<      Gravity's Rainbow
2026-03-25  6<      -0.86<      Manufacturing Consent (oops! fell of the wagon)
2026-03-28  7<      -0.29<      The Stranger
2026-04-01  8<       0.14<      The Silmarillion (back on track!)
```

Variants where you track a cumulative value that has varying increments, like kilometers run per day, or an absolute value, like body weight, are left as exercise.
