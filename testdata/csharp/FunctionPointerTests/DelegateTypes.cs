
class C
{
    delegate delegate* unmanaged[cdecl]<void> M(delegate*<ref C, D> p);
}