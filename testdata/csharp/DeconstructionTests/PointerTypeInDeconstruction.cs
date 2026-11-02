
class C
{
    void M()
    {
        // syntax error: pointer types only permitted as an array element type in a tuple
        (int* x1, int y1) = e;

        // These are OK, because an array is a valid type in a tuple.
        (int*[] x2, int y2) = e;
        (var*[] x3, int y3) = e;

        // Multiplication in a tuple element is also OK
        (var* x4, int y4) = e;
        (var* x5, var* y5) = e;
        e = (var* x6, var* y6);
    }
}
