
using System.Collections.Generic;
using System.Linq;
public class QueryExpressionTest
{
    public static int Main()
    {
        int[] expr1 = new int[] { 1, 2, 3, };
        IEnumerable<int> query01 = from value in expr1 select value;
        IEnumerable<int> query02 = from yield in expr1 select yield;
        IEnumerable<int> query03 = from select in expr1 select select;
        return 0;
    }
}