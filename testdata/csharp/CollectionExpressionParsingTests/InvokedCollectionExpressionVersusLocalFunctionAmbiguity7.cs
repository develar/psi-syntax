class C
{
    void M()
    {
        [() => {}][rand.Next()](A)[0] = 1;
    }
}