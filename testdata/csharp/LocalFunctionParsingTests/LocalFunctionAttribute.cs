
class C
{
    void M()
    {
        [A]
        void local() { }

        [return: A]
        void local() { }

        [A]
        int local() => 42;

        [A][B] void local() { }
    }
}