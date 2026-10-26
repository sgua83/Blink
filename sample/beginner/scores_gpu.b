module beginner.scores_gpu;
import beginner.scores;

u32 main() on cpu {
    dataset<Result> results = dataset<Result>(4);
    dataset<Number, 1> total = dataset<Number>(1);
    upload(results);
    upload(total);
    run_gpu(beginner.scores::run(results, total, 5));
    download(results);
    download(total);
    for (i in 0..results.length) {
        println(results[i].original);
        println(results[i].adjusted);
        println(results[i].passed);
    }
    println(total[0].value);
    return 0;
}
