class X<T>
{
}

class Y<A, B>
{
}

class C
{
    void M<T1, T2>()
    {
        M<X<>, Y<,>>();
    }
}