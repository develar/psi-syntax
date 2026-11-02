class Q
{
    static void Main()
    {
        var expr1 = new[] { 1, 2, 3, };
        var expr2 = new[] { 3, 4, 5, };
        var q = from i in (expr1) let j = expr2 select i;

        var qq = from x1 in new[] { 3, 9, 5, 5, 0, 8, 6, 8, 9, }
                  orderby x1 descending
                  let x47 = x1
                  select (x47) - (x1);

    }
}