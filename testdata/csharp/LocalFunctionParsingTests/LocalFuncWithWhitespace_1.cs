
class C
{
    void Main()
    {
        int
            goo() => 5;

        int
            goo() { return 5; }

        int
            goo<T>() => 5;

        int
            goo<T>() { return 5; }

        int
            goo<T>() where T : IFace => 5;

        int
            goo<T>() where T : IFace { return 5; }
    }
}