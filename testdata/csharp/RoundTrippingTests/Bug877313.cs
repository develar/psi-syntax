
partial class ConstraintsDef
{
    partial void PM<T1, T2>(T1 v1, ref T2 v2, params T2[] v3)
        where T2 : new()
        where T1 : System.IComparable<T1>, System.Collections.IEnumerable;
    partial void PM<T1, T2>(T1 v1, ref T2 v2, params T2[] v3)
    {
        v2 = v3[0];
    }
}
