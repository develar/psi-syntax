
using System;
using System.Linq;
public class C
{
    public static int Main()
    {
        int[] array1 = { 1, 2, 3 ,4, 5, 6,};
        int[] array2 = { 5, 6, 7, 8, 9 };
        var c = from x in array1
                join y in array2 on x equals y
                group x y;
        return 1;
    }
}
