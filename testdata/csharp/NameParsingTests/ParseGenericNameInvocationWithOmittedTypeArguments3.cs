class X<T>
{
}

class C
{
    void M<T1, T2>()
    {
        M<X<>, int>();
    }
}