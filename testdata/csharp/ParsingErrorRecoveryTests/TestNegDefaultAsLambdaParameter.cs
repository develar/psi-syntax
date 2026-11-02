class C
{
    delegate T Func<T>();
    delegate T Func<A0, T>(A0 a0);
    delegate T Func<A0, A1, T>(A0 a0, A1 a1);
    delegate T Func<A0, A1, A2, A3, T>(A0 a0, A1 a1, A2 a2, A3 a3);

    static void X()
    {
        // Func<int,int> f1      = (int @in) => 1;              // ok: @Keyword as parameter name
        Func<int,int> f2      = (int where, int from) => 1;  // ok: contextual keyword as parameter name
        Func<int,int> f3      = (int default) => 1;          // err: Keyword as parameter name
    }
}
