public class C
{
    public void M()
    {
        int sum0 = Sum(1, 2));

        void Local()
        {
            AnotherLocal());

            int sum1 = Sum(1, 2));
            int sum2 = Sum(1, 3));

            void AnotherLocal()
            {
                int x = sum2 + 2;
            }
        }
    }

    public static int Sum(int a, int b) => a + b;
}