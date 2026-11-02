class C
{
    void M()
    {
        [A, B] switch { _ => M() };
    }
}