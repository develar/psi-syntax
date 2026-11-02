
using System;
using System.Linq;

public class QueryExpressionTest
{
    public static void Main()
    {
        var expr1 = new[] { 1, 2, 3 };
        var expr2 = new[] { 1, 2, 3 };

        var query13 = from  const in expr1 join  i in expr2 on const equals i select new { const, i };
    }
}
