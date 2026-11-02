
using System;
using System.Collections.Generic;
using System.Linq;

class Program
{
    public static bool Method1(IEnumerable<int> f1)
    {
        return true;
    }

    static void Main(string[] args)
    {
        var numbers = new int[] { 4, 5 };
        var f1 = from num1 in new int[from num2 in numbers select num2] select num1; //not valid but this is only a parser test
    }
}