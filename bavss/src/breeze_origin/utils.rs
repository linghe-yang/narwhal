
pub fn transpose<T: Clone>(matrix: Vec<Vec<T>>) -> Vec<Vec<T>> {
    // 检查矩阵是否为空或不规则
    if matrix.is_empty() {
        return Vec::new();
    }

    let rows = matrix.len();
    let cols = matrix[0].len();

    // 检查矩阵是否规则（每行长度相同）
    if !matrix.iter().all(|row| row.len() == cols) {
        panic!("Matrix must have consistent row lengths");
    }

    // 创建转置矩阵
    let mut result = Vec::with_capacity(cols);
    for j in 0..cols {
        let mut new_row = Vec::with_capacity(rows);
        for i in 0..rows {
            new_row.push(matrix[i][j].clone());
        }
        result.push(new_row);
    }

    result
}
