class C
{
    void M()
    {
        [() => {}][rand.Next()]();
    }
}