namespace N
{
    // This type should be unaffected
    class C
    {
    }

    class Type
    {
    }

    // Two members that will move into the type above.
    private void M()
    {
    }

    public static Type operator+(Type t1, Type t2) => default;

    // Following type declaration
    class Type2
    {
    }

    private static implicit operator int(Type2 t) => 0;
}