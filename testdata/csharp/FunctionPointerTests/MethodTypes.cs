
class C
{
    public delegate*<int, string> M(delegate*<C, void> param1, delegate* unmanaged[cdecl]<D> param2) {}
}